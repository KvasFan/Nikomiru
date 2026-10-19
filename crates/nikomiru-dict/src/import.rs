// SPDX-License-Identifier: GPL-3.0-only

//! Safe, atomic and cancellable import of a Yomitan zip (DIC-010…DIC-015).
//!
//! The zip is checked entry by entry before anything is read (DIC-011); the
//! dictionary is written to `dicts/<id>.importing/` and the folder is renamed
//! to `dicts/<id>/` only after the commit (DIC-013), so an error, a
//! cancellation or a crash leaves nothing behind.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
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

/// Opens `zip` and checks every entry (DIC-011); returns the entries with the
/// single wrapper folder, if any, removed from their names.
fn checked_entries(zip: &mut zip::ZipArchive<File>) -> Result<Vec<Entry>, ImportError> {
    let mut entries = Vec::new();
    let mut total: u64 = 0;
    for index in 0..zip.len() {
        let file = zip
            .by_index_raw(index)
            .map_err(|e| ImportError::Zip(e.to_string()))?;
        let raw = file
            .name()
            .map_err(|e| ImportError::Zip(e.to_string()))?
            .replace('\\', "/");
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
    // a single wrapper folder (`dict/index.json`) is stripped (BUG-2053)
    if !entries.iter().any(|e| e.name == "index.json")
        && let Some(prefix) = entries
            .iter()
            .find(|e| e.name.ends_with("/index.json") && e.name.matches('/').count() == 1)
            .map(|e| e.name.trim_end_matches("index.json").to_string())
        && entries.iter().all(|e| e.name.starts_with(&prefix))
    {
        for e in &mut entries {
            e.name = e.name[prefix.len()..].to_string();
        }
    }
    Ok(entries)
}

/// Reads an entry, refusing more bytes than declared (a lying header is a bomb).
fn read_entry(zip: &mut zip::ZipArchive<File>, entry: &Entry) -> Result<Vec<u8>, ImportError> {
    let file = zip
        .by_index(entry.index)
        .map_err(|e| ImportError::Zip(e.to_string()))?;
    let mut bytes = Vec::with_capacity(entry.size.min(64 << 20) as usize);
    file.take(entry.size + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| ImportError::Zip(format!("{}: {e}", entry.name)))?;
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
    let mut zip = open_zip(path)?;
    let entries = checked_entries(&mut zip)?;
    let entry = entries
        .iter()
        .find(|e| e.name == "index.json")
        .ok_or(ImportError::NoIndex)?;
    let bytes = read_entry(&mut zip, entry)?;
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
    let result = write(path, &staging, cancel, &mut progress);
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

fn write(
    path: &Path,
    staging: &Path,
    cancel: &Cancel,
    progress: &mut impl FnMut(Progress),
) -> Result<DictInfo, ImportError> {
    let mut zip = open_zip(path)?;
    let entries = checked_entries(&mut zip)?;
    let index_entry = entries
        .iter()
        .find(|e| e.name == "index.json")
        .ok_or(ImportError::NoIndex)?;
    let index = yomitan::parse_index(&read_entry(&mut zip, index_entry)?).map_err(|error| {
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
    let term_banks = banks.keys().filter(|(b, _)| *b == Bank::Terms).count();
    let kanji_banks = banks.keys().filter(|(b, _)| *b == Bank::Kanji).count();
    let mut state = Progress {
        files_done: 0,
        files_total: banks.len() + media.len(),
        entries: 0,
    };
    progress(state);

    let mut writer = StoreWriter::create(&staging.join(DB_FILE))?;
    let mut first_meta: Option<&'static str> = None;
    let format = |file: &str| {
        let file = file.to_string();
        move |error| ImportError::Format { file, error }
    };
    for ((bank, _), entry) in &banks {
        if cancel.is_cancelled() {
            return Err(ImportError::Cancelled);
        }
        let bytes = read_entry(&mut zip, entry)?;
        let added = match bank {
            Bank::Terms => {
                let terms =
                    yomitan::parse_terms(&bytes, index.format).map_err(format(&entry.name))?;
                if let Some(t) = terms.iter().find(|t| t.glossary.len() > MAX_GLOSSARY) {
                    return Err(ImportError::GlossaryTooLarge {
                        expression: t.expression.clone(),
                    });
                }
                let n = terms.len();
                writer.add_terms(terms)?;
                n
            }
            Bank::TermMeta | Bank::KanjiMeta => {
                let meta = yomitan::parse_meta_bank(&bytes).map_err(format(&entry.name))?;
                if first_meta.is_none() {
                    first_meta = meta.first().map(|(_, m)| m.mode());
                }
                let n = meta.len();
                writer.add_meta(meta, *bank == Bank::KanjiMeta)?;
                n
            }
            Bank::Kanji => {
                let kanji =
                    yomitan::parse_kanji(&bytes, index.format).map_err(format(&entry.name))?;
                let n = kanji.len();
                writer.add_kanji(kanji)?;
                n
            }
            Bank::Tags => {
                let tags = yomitan::parse_tags(&bytes).map_err(format(&entry.name))?;
                let n = tags.len();
                writer.add_tags(tags)?;
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
        if cancel.is_cancelled() {
            return Err(ImportError::Cancelled);
        }
        let bytes = read_entry(&mut zip, entry)?;
        writer.add_media(&entry.name, &bytes)?;
        state.files_done += 1;
        progress(state);
    }
    let kinds = yomitan::detect_kinds(term_banks, kanji_banks, first_meta);
    if kinds == yomitan::Kinds::default() {
        return Err(ImportError::Empty);
    }
    let styles = match styles {
        Some(e) => String::from_utf8_lossy(&read_entry(&mut zip, e)?).into_owned(),
        None => String::new(),
    };
    if cancel.is_cancelled() {
        return Err(ImportError::Cancelled);
    }
    Ok(writer.finish(index, kinds, styles)?)
}

#[cfg(test)]
mod tests {
    use super::*;

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
