// SPDX-License-Identifier: GPL-3.0-only

//! Safe, atomic and cancellable import of a Yomitan zip (DIC-010…DIC-015).
//!
//! The zip is checked entry by entry before anything is read (DIC-011); the
//! dictionary is written to `dicts/<id>.importing/` and the folder is renamed
//! to `dicts/<id>/` only after the commit (DIC-013), so an error, a
//! cancellation or a crash leaves nothing behind.
//!
//! A cancel is honoured within a fraction of a second at every step, a single
//! JMdict-sized bank included: entries are read a chunk at a time, banks are
//! parsed and written a stretch of rows at a time, and the final step looks
//! at it too. Only redb's commit and compaction cannot be interrupted.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use nikomiru_core::Cancel;

use crate::store::{DictInfo, StoreError, StoreWriter};
use crate::yomitan::{self, FormatError, Index};

/// Largest uncompressed entry (DIC-011).
pub const MAX_ENTRY: u64 = 512 << 20;
/// Entries above this size must not be compressed more than [`MAX_RATIO`].
pub const RATIO_FLOOR: u64 = 1 << 20;
/// Largest compression ratio of an entry above [`RATIO_FLOOR`] (DIC-011).
pub const MAX_RATIO: u64 = 200;
/// Largest uncompressed total (DIC-011).
pub const MAX_TOTAL: u64 = 8 << 30;
/// Most entries in a dictionary (DIC-012).
pub const MAX_ENTRIES: u64 = 10_000_000;
/// Largest glossary of one entry (DIC-012).
pub const MAX_GLOSSARY: usize = 10 << 20;

/// File name of the dictionary database inside its folder.
pub const DB_FILE: &str = "dict.redb";
/// Suffix of a folder that is being written.
pub const IMPORTING: &str = ".importing";

/// Bytes of an entry decompressed between two looks at the cancel flag.
const READ_CHUNK: u64 = 1 << 20;
/// Rows of a bank written between two looks at the cancel flag.
const WRITE_CHUNK: usize = 4096;

/// Why an entry of the zip is refused (DIC-011).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unsafe {
    /// An absolute path.
    Absolute,
    /// A `..` component.
    Traversal,
    /// A symbolic link.
    Symlink,
    /// Larger than [`MAX_ENTRY`].
    TooLarge,
    /// Compressed more than [`MAX_RATIO`]:1, or larger than declared.
    Bomb,
    /// The entries add up to more than [`MAX_TOTAL`].
    TotalTooLarge,
}

/// Errors of an import.
#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    /// The zip or the dictionary folder could not be read or written.
    #[error("{path}: {source}")]
    Io {
        /// The file.
        path: PathBuf,
        /// The error.
        source: std::io::Error,
    },
    /// Not a readable zip.
    #[error("not a zip file: {0}")]
    Zip(String),
    /// A StarDict, DSL or MDict file that cannot be used (DIC-004).
    #[error("{file}: {reason}")]
    Foreign {
        /// The file.
        file: String,
        /// Why.
        reason: String,
    },
    /// An entry was refused.
    #[error("unsafe entry {entry}: {reason:?}")]
    Unsafe {
        /// The entry.
        entry: String,
        /// Why.
        reason: Unsafe,
    },
    /// No `index.json`.
    #[error("not a Yomitan dictionary: index.json is missing")]
    NoIndex,
    /// A bank is not valid JSON or not in the format.
    #[error("{file}: {error}")]
    Format {
        /// The bank.
        file: String,
        /// What is wrong.
        error: FormatError,
    },
    /// More than [`MAX_ENTRIES`].
    #[error("more than {MAX_ENTRIES} entries")]
    TooManyEntries,
    /// A glossary larger than [`MAX_GLOSSARY`].
    #[error("the glossary of {expression} is larger than 10 MiB")]
    GlossaryTooLarge {
        /// The entry.
        expression: String,
    },
    /// Nothing that a dictionary can contain.
    #[error("the dictionary is empty")]
    Empty,
    /// The disk is full (DIC-014, ERR_STORAGE_FULL).
    #[error("the disk is full")]
    StorageFull,
    /// Cancelled by the user (DIC-010).
    #[error("import cancelled")]
    Cancelled,
    /// The dictionary database failed.
    #[error(transparent)]
    Store(StoreError),
}

impl From<StoreError> for ImportError {
    fn from(e: StoreError) -> Self {
        if let StoreError::Db(redb::Error::Io(io)) = &e
            && io.kind() == std::io::ErrorKind::StorageFull
        {
            return ImportError::StorageFull;
        }
        ImportError::Store(e)
    }
}

impl nikomiru_core::Localized for ImportError {
    fn message_id(&self) -> &'static str {
        match self {
            ImportError::Io { .. } => "error-dict-io",
            ImportError::Zip(_) => "error-dict-zip",
            ImportError::Foreign { .. } => "error-dict-foreign",
            ImportError::Unsafe { .. } => "error-dict-unsafe",
            ImportError::NoIndex => "error-dict-no-index",
            ImportError::Format { .. } => "error-dict-format",
            ImportError::TooManyEntries => "error-dict-too-many",
            ImportError::GlossaryTooLarge { .. } => "error-dict-glossary",
            ImportError::Empty => "error-dict-empty",
            ImportError::StorageFull => "error-dict-storage-full",
            ImportError::Cancelled => "error-dict-cancelled",
            ImportError::Store(_) => "error-dict-store",
        }
    }

    fn message_args(&self) -> Vec<(&'static str, String)> {
        match self {
            ImportError::Io { path, source } => vec![
                ("path", path.display().to_string()),
                ("reason", source.to_string()),
            ],
            ImportError::Zip(reason) => vec![("reason", reason.clone())],
            ImportError::Foreign { file, reason } => {
                vec![("file", file.clone()), ("reason", reason.clone())]
            }
            ImportError::Unsafe { entry, .. } => vec![("entry", entry.clone())],
            ImportError::Format { file, error } => {
                vec![("file", file.clone()), ("reason", error.to_string())]
            }
            ImportError::GlossaryTooLarge { expression } => {
                vec![("expression", expression.clone())]
            }
            ImportError::Store(e) => vec![("reason", e.to_string())],
            ImportError::NoIndex
            | ImportError::TooManyEntries
            | ImportError::Empty
            | ImportError::StorageFull
            | ImportError::Cancelled => Vec::new(),
        }
    }
}

/// Progress of an import (DIC-010).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Progress {
    /// Files read.
    pub files_done: usize,
    /// Files to read.
    pub files_total: usize,
    /// Entries written.
    pub entries: u64,
}

/// A finished import.
#[derive(Debug, Clone)]
pub struct Imported {
    /// The new dictionary's id (its folder name).
    pub id: String,
    /// Its folder.
    pub dir: PathBuf,
    /// What it contains.
    pub info: DictInfo,
}

/// The kind of a zip entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Bank {
    Tags,
    Terms,
    TermMeta,
    Kanji,
    KanjiMeta,
}

/// `term_bank_12.json` → (Terms, 12).
fn bank_of(name: &str) -> Option<(Bank, u32)> {
    let stem = name.strip_suffix(".json")?;
    let (kind, number) = stem.rsplit_once('_')?;
    let number = number.parse().ok()?;
    let bank = match kind {
        "term_bank" => Bank::Terms,
        "term_meta_bank" => Bank::TermMeta,
        "kanji_bank" => Bank::Kanji,
        "kanji_meta_bank" => Bank::KanjiMeta,
        "tag_bank" => Bank::Tags,
        _ => return None,
    };
    Some((bank, number))
}

/// A checked entry: its index in the zip and its name inside the dictionary.
#[derive(Debug, Clone)]
struct Entry {
    index: usize,
    name: String,
    size: u64,
}

fn ignored(name: &str) -> bool {
    name.starts_with("__MACOSX/")
        || name
            .split('/')
            .next_back()
            .is_some_and(|f| f == ".DS_Store")
        || name.ends_with('/')
}

/// CP437, the encoding of a zip name without the UTF-8 flag: its upper half.
const CP437: &str = "ÇüéâäàåçêëèïîìÄÅÉæÆôöòûùÿÖÜ¢£¥₧ƒáíóúñÑªº¿⌐¬½¼¡«»░▒▓│┤╡╢╖╕╣║╗╝╜╛┐└┴┬├─┼╞╟╚╔╩╦╠═╬╧╨╤╥╙╘╒╓╫╪┘┌█▄▌▐▀αßΓπΣσµτΦΘΩδ∞φε∩≡±≥≤⌠⌡÷≈°∙·√ⁿ²■\u{a0}";

/// The CRC-32 of zip (IEEE, reflected).
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &b in bytes {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// The name of an entry from its central header: the raw name, its flags and
/// its extra block (DIC-011, APPNOTE 4.6.9, like Fushi's
/// `zip_unicode_path_extra_test.cpp`). The Info-ZIP Unicode Path field
/// (0x7075) gives the name only when the UTF-8 flag (bit 11) is clear, its
/// version is 1, its CRC-32 is the raw name's and its name is UTF-8 and not
/// empty; a record that overruns the block ends the search. Otherwise the raw
/// name: UTF-8 with the flag or when it is valid UTF-8, else CP437.
fn entry_name(raw: &[u8], flags: u16, extra: &[u8]) -> String {
    const UTF8: u16 = 1 << 11;
    let from_field = (flags & UTF8 == 0)
        .then(|| {
            let mut rest = extra;
            while let [a, b, c, d, tail @ ..] = rest {
                let (id, len) = (
                    u16::from_le_bytes([*a, *b]),
                    usize::from(u16::from_le_bytes([*c, *d])),
                );
                let data = tail.get(..len)?;
                if id == 0x7075
                    && let [1, c0, c1, c2, c3, name @ ..] = data
                    && u32::from_le_bytes([*c0, *c1, *c2, *c3]) == crc32(raw)
                    && !name.is_empty()
                    && let Ok(name) = std::str::from_utf8(name)
                {
                    return Some(name.to_string());
                }
                rest = &tail[len..];
            }
            None
        })
        .flatten();
    from_field.unwrap_or_else(|| match std::str::from_utf8(raw) {
        Ok(name) => name.to_string(),
        Err(_) if flags & UTF8 != 0 => String::from_utf8_lossy(raw).into_owned(),
        Err(_) => raw
            .iter()
            .map(|&b| {
                if b < 0x80 {
                    char::from(b)
                } else {
                    CP437.chars().nth(usize::from(b - 0x80)).unwrap_or('?')
                }
            })
            .collect(),
    })
}

/// Reads the central header at `offset` of `file` and gives its entry's
/// name by [`entry_name`]: the zip crate applies the Unicode Path field by
/// rules of its own, and keeps no raw name once it has.
fn central_name(file: &mut File, offset: u64) -> Result<String, ImportError> {
    let zip_error = |e: std::io::Error| ImportError::Zip(e.to_string());
    let mut header = [0u8; 46];
    file.seek(SeekFrom::Start(offset)).map_err(zip_error)?;
    file.read_exact(&mut header).map_err(zip_error)?;
    if header[..4] != [0x50, 0x4b, 0x01, 0x02] {
        return Err(ImportError::Zip("not a central directory header".into()));
    }
    let u16_at = |i: usize| u16::from_le_bytes([header[i], header[i + 1]]);
    let (flags, name_len, extra_len) =
        (u16_at(8), usize::from(u16_at(28)), usize::from(u16_at(30)));
    let mut bytes = vec![0u8; name_len + extra_len];
    file.read_exact(&mut bytes).map_err(zip_error)?;
    Ok(entry_name(&bytes[..name_len], flags, &bytes[name_len..]))
}

/// Opens `zip` and checks every entry (DIC-011); returns the entries with the
/// wrapper folders, if any, removed from their names. `path` is the zip's,
/// read again for the raw names.
fn checked_entries(
    zip: &mut zip::ZipArchive<File>,
    path: &Path,
) -> Result<Vec<Entry>, ImportError> {
    let mut headers = File::open(path).map_err(|source| ImportError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut entries = Vec::new();
    let mut total: u64 = 0;
    for index in 0..zip.len() {
        let file = zip
            .by_index_raw(index)
            .map_err(|e| ImportError::Zip(e.to_string()))?;
        let raw = central_name(&mut headers, file.central_header_start())?.replace('\\', "/");
        let refuse = |reason| ImportError::Unsafe {
            entry: raw.clone(),
            reason,
        };
        if raw.starts_with('/') || raw.as_bytes().get(1) == Some(&b':') {
            return Err(refuse(Unsafe::Absolute));
        }
        if raw.split('/').any(|c| c == "..") || file.enclosed_name().is_none() {
            return Err(refuse(Unsafe::Traversal));
        }
        if file.is_symlink() {
            return Err(refuse(Unsafe::Symlink));
        }
        if ignored(&raw) {
            continue;
        }
        let size = file.size();
        if size > MAX_ENTRY {
            return Err(refuse(Unsafe::TooLarge));
        }
        if size > RATIO_FLOOR && size > file.compressed_size().saturating_mul(MAX_RATIO) {
            return Err(refuse(Unsafe::Bomb));
        }
        total = total.saturating_add(size);
        if total > MAX_TOTAL {
            return Err(refuse(Unsafe::TotalTooLarge));
        }
        entries.push(Entry {
            index,
            name: raw,
            size,
        });
    }
    // a folder holding the whole archive (`dict/index.json`) is stripped,
    // again while index.json is not at the root (BUG-2053)
    while !entries.iter().any(|e| e.name == "index.json")
        && let Some(prefix) = entries
            .first()
            .and_then(|e| e.name.split_once('/'))
            .map(|(folder, _)| format!("{folder}/"))
        && entries.iter().all(|e| e.name.starts_with(&prefix))
    {
        for e in &mut entries {
            e.name = e.name[prefix.len()..].to_string();
        }
    }
    Ok(entries)
}

/// Reads an entry, refusing more bytes than declared (a lying header is a
/// bomb), a [`READ_CHUNK`] at a time so that a cancel stops a large one.
fn read_entry(
    zip: &mut zip::ZipArchive<File>,
    entry: &Entry,
    cancel: &Cancel,
) -> Result<Vec<u8>, ImportError> {
    let file = zip
        .by_index(entry.index)
        .map_err(|e| ImportError::Zip(e.to_string()))?;
    let mut bytes = Vec::with_capacity(entry.size.min(64 << 20) as usize);
    let mut file = file.take(entry.size + 1);
    loop {
        if cancel.is_cancelled() {
            return Err(ImportError::Cancelled);
        }
        let read = (&mut file)
            .take(READ_CHUNK)
            .read_to_end(&mut bytes)
            .map_err(|e| ImportError::Zip(format!("{}: {e}", entry.name)))?;
        if read == 0 {
            break;
        }
    }
    if bytes.len() as u64 > entry.size {
        return Err(ImportError::Unsafe {
            entry: entry.name.clone(),
            reason: Unsafe::Bomb,
        });
    }
    Ok(bytes)
}

fn open_zip(path: &Path) -> Result<zip::ZipArchive<File>, ImportError> {
    let file = File::open(path).map_err(|source| ImportError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    zip::ZipArchive::new(file).map_err(|e| ImportError::Zip(e.to_string()))
}

/// The `index.json` of a zip, to compare title and revision with the
/// installed dictionaries before importing (DIC-016).
pub fn read_index(path: &Path) -> Result<Index, ImportError> {
    if let Some(kind) = crate::foreign::kind_of(path) {
        return crate::foreign::read_index(path, kind);
    }
    let mut zip = open_zip(path)?;
    let entries = checked_entries(&mut zip, path)?;
    let entry = entries
        .iter()
        .find(|e| e.name == "index.json")
        .ok_or(ImportError::NoIndex)?;
    let bytes = read_entry(&mut zip, entry, &Cancel::new())?;
    yomitan::parse_index(&bytes).map_err(|error| ImportError::Format {
        file: "index.json".into(),
        error,
    })
}

/// Removes the folders that an interrupted import left (DIC-013); returns how
/// many.
pub fn cleanup(dicts_dir: &Path) -> std::io::Result<usize> {
    let Ok(read) = std::fs::read_dir(dicts_dir) else {
        return Ok(0);
    };
    let mut removed = 0;
    for entry in read.flatten() {
        if entry.file_name().to_string_lossy().ends_with(IMPORTING) {
            std::fs::remove_dir_all(entry.path())?;
            removed += 1;
        }
    }
    Ok(removed)
}

/// Imports the zip at `path` into a new folder of `dicts_dir`.
pub fn import(
    path: &Path,
    dicts_dir: &Path,
    cancel: &Cancel,
    mut progress: impl FnMut(Progress),
) -> Result<Imported, ImportError> {
    let id = uuid::Uuid::new_v4().simple().to_string();
    let staging = dicts_dir.join(format!("{id}{IMPORTING}"));
    let io = |path: &Path| {
        let path = path.to_path_buf();
        move |source: std::io::Error| match source.kind() {
            std::io::ErrorKind::StorageFull => ImportError::StorageFull,
            _ => ImportError::Io { path, source },
        }
    };
    std::fs::create_dir_all(&staging).map_err(io(&staging))?;
    let result = write(path, &staging, cancel, &mut progress).and_then(|info| {
        // a cancel that came during the commit or the compaction still counts
        if cancel.is_cancelled() {
            Err(ImportError::Cancelled)
        } else {
            Ok(info)
        }
    });
    match result {
        Ok(info) => {
            let dir = dicts_dir.join(&id);
            if let Err(e) = std::fs::rename(&staging, &dir) {
                let _ = std::fs::remove_dir_all(&staging);
                return Err(io(&dir)(e));
            }
            Ok(Imported { id, dir, info })
        }
        Err(e) => {
            let _ = std::fs::remove_dir_all(&staging);
            Err(e)
        }
    }
}

/// The rows of a parsed bank; the parser gives `None` when the cancel
/// stopped it.
fn parsed<T>(file: &str, rows: Result<Option<Vec<T>>, FormatError>) -> Result<Vec<T>, ImportError> {
    match rows {
        Ok(Some(rows)) => Ok(rows),
        Ok(None) => Err(ImportError::Cancelled),
        Err(error) => Err(ImportError::Format {
            file: file.to_string(),
            error,
        }),
    }
}

/// A StarDict, DSL or MDict file, into the same store (DIC-004).
fn write_foreign(
    path: &Path,
    kind: crate::foreign::Kind,
    staging: &Path,
    cancel: &Cancel,
    progress: &mut impl FnMut(Progress),
) -> Result<DictInfo, ImportError> {
    let index = crate::foreign::read_index(path, kind)?;
    let mut state = Progress {
        files_done: 0,
        files_total: 1,
        entries: 0,
    };
    progress(state);
    let terms = crate::foreign::read_terms(path, kind, cancel)?;
    if terms.is_empty() {
        return Err(ImportError::Empty);
    }
    state.entries = terms.len() as u64;
    let mut writer = StoreWriter::create(&staging.join(DB_FILE))?;
    write_rows(terms, cancel, |rows| writer.add_terms(rows))?;
    state.files_done = 1;
    progress(state);
    let kinds = yomitan::detect_kinds(yomitan::Rows {
        terms: writer.counts().terms,
        kanji: 0,
        frequencies: 0,
        pitches: 0,
    });
    writer
        .finish(index, kinds, String::new(), cancel)?
        .ok_or(ImportError::Cancelled)
}

/// Writes `rows` to the store a [`WRITE_CHUNK`] at a time, looking at
/// `cancel` between chunks: a large bank takes seconds to write.
fn write_rows<T>(
    rows: Vec<T>,
    cancel: &Cancel,
    mut write: impl FnMut(Vec<T>) -> Result<(), StoreError>,
) -> Result<(), ImportError> {
    let mut rows = rows.into_iter();
    loop {
        if cancel.is_cancelled() {
            return Err(ImportError::Cancelled);
        }
        let chunk: Vec<T> = rows.by_ref().take(WRITE_CHUNK).collect();
        if chunk.is_empty() {
            return Ok(());
        }
        write(chunk)?;
    }
}

fn write(
    path: &Path,
    staging: &Path,
    cancel: &Cancel,
    progress: &mut impl FnMut(Progress),
) -> Result<DictInfo, ImportError> {
    if let Some(kind) = crate::foreign::kind_of(path) {
        return write_foreign(path, kind, staging, cancel, progress);
    }
    let mut zip = open_zip(path)?;
    let entries = checked_entries(&mut zip, path)?;
    let index_entry = entries
        .iter()
        .find(|e| e.name == "index.json")
        .ok_or(ImportError::NoIndex)?;
    let index =
        yomitan::parse_index(&read_entry(&mut zip, index_entry, cancel)?).map_err(|error| {
            ImportError::Format {
                file: "index.json".into(),
                error,
            }
        })?;
    let mut banks: BTreeMap<(Bank, u32), &Entry> = BTreeMap::new();
    let mut styles: Option<&Entry> = None;
    let mut media: Vec<&Entry> = Vec::new();
    for e in &entries {
        if e.name == "index.json" {
            continue;
        }
        if e.name == "styles.css" {
            styles = Some(e);
        } else if let Some(key) = bank_of(&e.name).filter(|_| !e.name.contains('/')) {
            banks.insert(key, e);
        } else {
            media.push(e);
        }
    }
    let mut state = Progress {
        files_done: 0,
        files_total: banks.len() + media.len(),
        entries: 0,
    };
    progress(state);

    let mut writer = StoreWriter::create(&staging.join(DB_FILE))?;
    for ((bank, _), entry) in &banks {
        let bytes = read_entry(&mut zip, entry, cancel)?;
        let name = entry.name.as_str();
        let added = match bank {
            Bank::Terms => {
                let terms = parsed(name, yomitan::parse_terms(&bytes, index.format, cancel))?;
                if let Some(t) = terms.iter().find(|t| t.glossary.len() > MAX_GLOSSARY) {
                    return Err(ImportError::GlossaryTooLarge {
                        expression: t.expression.clone(),
                    });
                }
                let n = terms.len();
                write_rows(terms, cancel, |rows| writer.add_terms(rows))?;
                n
            }
            Bank::TermMeta | Bank::KanjiMeta => {
                let meta = parsed(name, yomitan::parse_meta_bank(&bytes, cancel))?;
                let n = meta.len();
                let kanji = *bank == Bank::KanjiMeta;
                write_rows(meta, cancel, |rows| writer.add_meta(rows, kanji))?;
                n
            }
            Bank::Kanji => {
                let kanji = parsed(name, yomitan::parse_kanji(&bytes, index.format, cancel))?;
                let n = kanji.len();
                write_rows(kanji, cancel, |rows| writer.add_kanji(rows))?;
                n
            }
            Bank::Tags => {
                let tags = parsed(name, yomitan::parse_tags(&bytes, cancel))?;
                let n = tags.len();
                write_rows(tags, cancel, |rows| writer.add_tags(rows))?;
                n
            }
        };
        state.entries += added as u64;
        if state.entries > MAX_ENTRIES {
            return Err(ImportError::TooManyEntries);
        }
        state.files_done += 1;
        progress(state);
    }
    for entry in media {
        let bytes = read_entry(&mut zip, entry, cancel)?;
        writer.add_media(&entry.name, &bytes)?;
        state.files_done += 1;
        progress(state);
    }
    let counts = writer.counts();
    let kinds = yomitan::detect_kinds(yomitan::Rows {
        terms: counts.terms,
        kanji: counts.kanji,
        // kanji meta is only frequencies
        frequencies: counts.frequencies + counts.kanji_meta,
        pitches: counts.pitches,
    });
    if kinds == yomitan::Kinds::default() {
        return Err(ImportError::Empty);
    }
    let styles = match styles {
        Some(e) => String::from_utf8_lossy(&read_entry(&mut zip, e, cancel)?).into_owned(),
        None => String::new(),
    };
    writer
        .finish(index, kinds, styles, cancel)?
        .ok_or(ImportError::Cancelled)
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-DIC-011
    #[test]
    fn zip_names_follow_the_unicode_path_rules() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        // Shift-JIS 一 (0x88 0xEA) without the flag: CP437
        let raw = b"a/\x88\xea.svg";
        assert_eq!(entry_name(raw, 0, &[]), "a/êΩ.svg");
        let field = |version: u8, crc: u32, name: &str| {
            let mut data = vec![version];
            data.extend(crc.to_le_bytes());
            data.extend(name.as_bytes());
            let mut block = vec![0x75, 0x70];
            block.extend((data.len() as u16).to_le_bytes());
            block.extend(data);
            block
        };
        let good = field(1, crc32(raw), "a/一.svg");
        assert_eq!(entry_name(raw, 0, &good), "a/一.svg");
        assert_eq!(
            entry_name(raw, 1 << 11, &good),
            "a/\u{fffd}\u{fffd}.svg",
            "the flag wins"
        );
        assert_eq!(
            entry_name(raw, 0, &field(2, crc32(raw), "a/一.svg")),
            "a/êΩ.svg"
        );
        assert_eq!(entry_name(raw, 0, &field(1, 7, "a/一.svg")), "a/êΩ.svg");
        assert_eq!(entry_name(raw, 0, &field(1, crc32(raw), "")), "a/êΩ.svg");
        // after another record, and an overrunning one ends the search
        let mut second = vec![0x55, 0x54, 1, 0, 0];
        second.extend(&good);
        assert_eq!(entry_name(raw, 0, &second), "a/一.svg");
        assert_eq!(entry_name(raw, 0, &[0x75, 0x70, 200, 0, 1]), "a/êΩ.svg");
        assert_eq!(entry_name("b/日.txt".as_bytes(), 0, &[]), "b/日.txt");
    }

    #[test]
    fn bank_names() {
        assert_eq!(bank_of("term_bank_12.json"), Some((Bank::Terms, 12)));
        assert_eq!(bank_of("term_meta_bank_1.json"), Some((Bank::TermMeta, 1)));
        assert_eq!(
            bank_of("kanji_meta_bank_3.json"),
            Some((Bank::KanjiMeta, 3))
        );
        assert_eq!(bank_of("tag_bank_1.json"), Some((Bank::Tags, 1)));
        assert_eq!(bank_of("img/a.png"), None);
        assert_eq!(bank_of("term_bank_x.json"), None);
        assert!(
            ignored("__MACOSX/._term_bank_1.json")
                && ignored("a/.DS_Store")
                && !ignored("img/a.png")
        );
    }

    // T-DIC-010
    #[test]
    fn rows_are_written_a_chunk_at_a_time_until_a_cancel() {
        let cancel = Cancel::new();
        let mut chunks = Vec::new();
        let stopped = write_rows((0..3 * WRITE_CHUNK).collect(), &cancel, |rows| {
            chunks.push(rows.len());
            cancel.cancel();
            Ok(())
        });
        assert!(matches!(stopped, Err(ImportError::Cancelled)));
        assert_eq!(chunks, [WRITE_CHUNK], "the rest is not written");
        let mut written = Vec::new();
        write_rows((0..2 * WRITE_CHUNK + 1).collect(), &Cancel::new(), |rows| {
            written.extend(rows);
            Ok(())
        })
        .unwrap();
        assert_eq!(written, (0..2 * WRITE_CHUNK + 1).collect::<Vec<_>>());
    }

    // T-ERR-001
    #[test]
    fn every_error_has_its_messages() {
        use nikomiru_core::Localized;
        let errors = [
            ImportError::Io {
                path: "/x.zip".into(),
                source: std::io::Error::other("x"),
            },
            ImportError::Zip("x".into()),
            ImportError::Unsafe {
                entry: "../x".into(),
                reason: Unsafe::Traversal,
            },
            ImportError::NoIndex,
            ImportError::Format {
                file: "term_bank_1.json".into(),
                error: FormatError("x".into()),
            },
            ImportError::TooManyEntries,
            ImportError::GlossaryTooLarge {
                expression: "x".into(),
            },
            ImportError::Empty,
            ImportError::StorageFull,
            ImportError::Cancelled,
            ImportError::Store(StoreError::Glossary("x".into())),
        ];
        let all: Vec<&dyn Localized> = errors.iter().map(|e| e as _).collect();
        let dir = Path::new(nikomiru_core::error::I18N_DIR);
        let problems = nikomiru_core::error::message_problems(dir, &all);
        assert!(problems.is_empty(), "{problems:#?}");
    }
}
