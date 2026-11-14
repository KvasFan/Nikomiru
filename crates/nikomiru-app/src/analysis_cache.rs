// SPDX-License-Identifier: GPL-3.0-only

//! Bounded private completed-count caches (KNW-025), used only on workers.

use nikomiru_core::Cancel;
use nikomiru_library::{DictionaryManager, words::Counted};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

static CACHE_IO: std::sync::Mutex<()> = std::sync::Mutex::new(());

const ENTRY_BYTES: u64 = 4 << 20;
const SOURCE_BYTES: u64 = 32 << 20;

#[derive(Serialize, Deserialize)]
struct Entry {
    format: u32,
    key: String,
    counted: Counted,
}

fn owned(name: &str) -> bool {
    fn digest(s: &str) -> bool {
        s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
    }
    name.strip_suffix(".json").is_some_and(digest)
        || name
            .strip_prefix('.')
            .and_then(|s| s.split_once(".json.tmp-"))
            .is_some_and(|(hash, pid)| {
                digest(hash) && !pid.is_empty() && pid.bytes().all(|b| b.is_ascii_digit())
            })
}

fn files(root: &Path) -> std::io::Result<Vec<(PathBuf, u64)>> {
    if std::fs::symlink_metadata(root).is_ok_and(|meta| meta.file_type().is_symlink()) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "count-cache root is a symlink",
        ));
    }
    let mut files = Vec::new();
    for name in ["books", "episodes"] {
        let dir = root.join(name);
        let meta = match std::fs::symlink_metadata(&dir) {
            Ok(meta) => meta,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e),
        };
        if !meta.is_dir() || meta.file_type().is_symlink() {
            continue;
        }
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            if entry.file_type()?.is_file() && owned(&entry.file_name().to_string_lossy()) {
                files.push((entry.path(), entry.metadata()?.len()));
            }
        }
    }
    Ok(files)
}

/// Worker-side snapshot of recognised book/episode count files, not other caches.
pub fn usage(root: &Path) -> std::io::Result<nikomiru_ui::catalog::AnalysisCacheUsage> {
    let _guard = CACHE_IO
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let files = files(root)?;
    Ok(nikomiru_ui::catalog::AnalysisCacheUsage {
        files: files.len(),
        bytes: files.iter().map(|(_, n)| *n).sum(),
    })
}

/// Remove only recognised count files/temporaries. Future completed work may refill them.
pub fn clear(root: &Path) -> std::io::Result<()> {
    let _guard = CACHE_IO
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    for (path, _) in files(root)? {
        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// Stable source/content/dictionary identity, or no caching for unreadable/large sources.
/// Content hashes catch changed bytes even when size and mtime were preserved.
pub fn key(
    manager: &DictionaryManager,
    source: &Path,
    identity: &str,
    cancel: &Cancel,
) -> Option<String> {
    let dictionary = manager.analysis_identity().ok()?;
    source_key(source, identity, &dictionary, cancel)
}

fn source_key(source: &Path, identity: &str, dictionary: &[u8], cancel: &Cancel) -> Option<String> {
    if cancel.is_cancelled() {
        return None;
    }
    let mut file = std::fs::File::open(source).ok()?;
    let before = file.metadata().ok()?;
    if !before.is_file() || before.len() > SOURCE_BYTES {
        return None;
    }
    let mut hash = Sha256::new();
    hash.update(b"nikomiru-count-v1");
    hash.update(env!("CARGO_PKG_VERSION"));
    hash.update((dictionary.len() as u64).to_le_bytes());
    hash.update(dictionary);
    hash.update((identity.len() as u64).to_le_bytes());
    hash.update(identity);
    hash.update(source.as_os_str().as_encoded_bytes());
    hash.update(before.len().to_le_bytes());
    let mtime = before.modified().ok()?;
    hash.update(
        mtime
            .duration_since(UNIX_EPOCH)
            .ok()?
            .as_nanos()
            .to_le_bytes(),
    );
    let mut bytes = 0u64;
    let mut buffer = [0u8; 64 << 10];
    loop {
        if cancel.is_cancelled() {
            return None;
        }
        let read = file.read(&mut buffer).ok()?;
        if read == 0 {
            break;
        }
        bytes += read as u64;
        if bytes > SOURCE_BYTES {
            return None;
        }
        hash.update(&buffer[..read]);
    }
    let after = std::fs::metadata(source).ok()?;
    if bytes != before.len() || after.len() != before.len() || after.modified().ok()? != mtime {
        return None;
    }
    Some(hex(&hash.finalize()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn valid(counted: &Counted) -> bool {
    counted.words.len() <= 100_000
        && counted.lines.len() <= 100_000
        && counted
            .words
            .iter()
            .all(|(e, r)| e.len() <= 64 << 10 && r.len() <= 64 << 10)
        && counted
            .lines
            .iter()
            .flatten()
            .all(|&i| (i as usize) < counted.words.len())
}

/// One worker's cache: 128 entries and 32 MiB; books and episodes have separate quotas.
#[derive(Debug)]
pub struct Store {
    dir: PathBuf,
    entries: usize,
    bytes: u64,
}

impl Store {
    /// Open a private count subdirectory and prune previous oversized caches.
    pub fn new(dir: PathBuf) -> Self {
        let store = Self {
            dir,
            entries: 128,
            bytes: 32 << 20,
        };
        store.prune();
        store
    }

    fn path(&self, key: &str) -> PathBuf {
        self.dir
            .join(format!("{}.json", hex(&Sha256::digest(key.as_bytes()))))
    }

    /// A complete validated count, or a cache miss. Bad entries are discarded.
    pub fn get(&self, key: &str, cancel: &Cancel) -> Option<Counted> {
        if cancel.is_cancelled() {
            return None;
        }
        let path = self.path(key);
        let read = || -> Option<Counted> {
            let file = std::fs::File::open(&path).ok()?;
            let meta = file.metadata().ok()?;
            if !meta.is_file() || meta.len() > ENTRY_BYTES {
                return None;
            }
            let mut bytes = Vec::new();
            file.take(ENTRY_BYTES + 1).read_to_end(&mut bytes).ok()?;
            if bytes.len() as u64 > ENTRY_BYTES || cancel.is_cancelled() {
                return None;
            }
            let entry: Entry = serde_json::from_slice(&bytes).ok()?;
            (entry.format == 1 && entry.key == key && valid(&entry.counted))
                .then_some(entry.counted)
        };
        match read() {
            Some(counted) => {
                if let Ok(file) = std::fs::OpenOptions::new().write(true).open(&path) {
                    let _ =
                        file.set_times(std::fs::FileTimes::new().set_modified(SystemTime::now()));
                }
                Some(counted)
            }
            None => {
                if !cancel.is_cancelled() {
                    let _ = std::fs::remove_file(path);
                }
                None
            }
        }
    }

    /// Commit only complete uncancelled counts; cache failures are optional.
    pub fn put(&self, key: &str, counted: &Counted, cancel: &Cancel) {
        if cancel.is_cancelled() || !valid(counted) {
            return;
        }
        let entry = Entry {
            format: 1,
            key: key.into(),
            counted: counted.clone(),
        };
        let Ok(bytes) = serde_json::to_vec(&entry) else {
            return;
        };
        if bytes.len() as u64 > ENTRY_BYTES || cancel.is_cancelled() {
            return;
        }
        let _guard = CACHE_IO
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let path = self.path(key);
        if nikomiru_core::fs::write_atomic(&path, &bytes, 0o600).is_ok() && cancel.is_cancelled() {
            let _ = std::fs::remove_file(path);
        }
        self.prune_locked();
    }

    fn prune(&self) {
        let _guard = CACHE_IO
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.prune_locked();
    }

    fn prune_locked(&self) {
        if std::fs::symlink_metadata(&self.dir).is_ok_and(|meta| meta.file_type().is_symlink()) {
            return;
        }
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return;
        };
        let mut files: Vec<_> = entries
            .flatten()
            .filter_map(|e| {
                let name = e.file_name();
                let name = name.to_string_lossy();
                if !owned(&name) || !e.file_type().ok()?.is_file() {
                    return None;
                }
                if name.starts_with('.') {
                    let _ = std::fs::remove_file(e.path());
                    return None;
                }
                let meta = e.metadata().ok()?;
                Some((e.path(), meta.len(), meta.modified().unwrap_or(UNIX_EPOCH)))
            })
            .collect();
        files.sort_by(|a, b| a.2.cmp(&b.2).then_with(|| a.0.cmp(&b.0)));
        let mut bytes: u64 = files.iter().map(|(_, size, _)| *size).sum();
        let mut count = files.len();
        for (path, size, _) in files {
            if count <= self.entries && bytes <= self.bytes && size <= ENTRY_BYTES {
                continue;
            }
            if std::fs::remove_file(path).is_ok() {
                count -= 1;
                bytes -= size;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn count() -> Counted {
        Counted::from_lines([vec![("猫".into(), "ねこ".into()); 3]])
    }

    // T-KNW-032: usage/clear/pruning recognise only owned names and preserve unrelated data.
    #[test]
    fn cache_management_cleans_owned_counts_and_interrupted_writes_only() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("counts-v1");
        let store = Store::new(root.join("books"));
        store.put("book", &count(), &Cancel::new());
        let keep = root.join("books/readme.json");
        std::fs::write(&keep, "keep").unwrap();
        let graphic = dir.path().join("subtitles.srt");
        std::fs::write(&graphic, "keep subtitle").unwrap();
        let temporary = root.join(format!("books/.{}.json.tmp-1234", "a".repeat(64)));
        std::fs::write(&temporary, "interrupted").unwrap();
        assert_eq!(usage(&root).unwrap().files, 2);
        assert!(usage(&root).unwrap().bytes > 11);
        let store = Store::new(root.join("books"));
        assert!(!temporary.exists());
        assert_eq!(usage(&root).unwrap().files, 1);
        clear(&root).unwrap();
        assert_eq!(usage(&root).unwrap().files, 0);
        assert_eq!(std::fs::read_to_string(keep).unwrap(), "keep");
        assert!(graphic.exists());
        store.put("again", &count(), &Cancel::new());
        assert_eq!(usage(&root).unwrap().files, 1);
    }

    // T-KNW-032: directory and file links never turn Clear into outside deletion.
    #[cfg(unix)]
    #[test]
    fn cache_clear_does_not_follow_links() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        let outside = dir.path().join("outside");
        std::fs::create_dir(&outside).unwrap();
        let target = outside.join(format!("{}.json", "a".repeat(64)));
        std::fs::write(&target, "keep").unwrap();
        let root = dir.path().join("counts-v1");
        std::fs::create_dir(&root).unwrap();
        symlink(&outside, root.join("episodes")).unwrap();
        std::fs::create_dir(root.join("books")).unwrap();
        symlink(&target, root.join(format!("books/{}.json", "b".repeat(64)))).unwrap();
        clear(&root).unwrap();
        assert!(target.exists());
        assert_eq!(usage(&root).unwrap().files, 0);
        let link = dir.path().join("root-link");
        symlink(&root, &link).unwrap();
        assert!(clear(&link).is_err());
        assert!(target.exists());
    }

    // T-KNW-025: source bytes, dictionary/language identity, path and cancellation.
    #[test]
    fn fingerprints_detect_changed_bytes_even_with_the_same_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("book.html");
        std::fs::write(&path, "食べる").unwrap();
        let stamp = std::fs::metadata(&path).unwrap().modified().unwrap();
        let cancel = Cancel::new();
        let original = source_key(&path, "book/1", b"dictionary-ja-v1", &cancel).unwrap();
        std::fs::write(&path, "日本語").unwrap();
        std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(stamp))
            .unwrap();
        let changed = source_key(&path, "book/1", b"dictionary-ja-v1", &cancel).unwrap();
        assert_ne!(original, changed);
        assert_ne!(
            changed,
            source_key(&path, "book/1", b"dictionary-en-v1", &cancel).unwrap()
        );
        assert_ne!(
            changed,
            source_key(&path, "book/2", b"dictionary-ja-v1", &cancel).unwrap()
        );
        let large = std::fs::File::create(&path).unwrap();
        large.set_len(SOURCE_BYTES + 1).unwrap();
        assert!(source_key(&path, "book/1", b"dict", &cancel).is_none());
        cancel.cancel();
        assert!(source_key(&path, "book/1", b"dict", &cancel).is_none());
    }

    // T-KNW-025: restart hit, corruption/format/index/size misses, no cancelled writes.
    #[test]
    fn complete_counts_survive_restart_and_bad_entries_are_misses() {
        let dir = tempfile::tempdir().unwrap();
        let cancel = Cancel::new();
        let store = Store::new(dir.path().to_path_buf());
        store.put("valid", &count(), &cancel);
        let reopened = Store::new(dir.path().to_path_buf());
        assert_eq!(reopened.get("valid", &cancel), Some(count()));
        for bytes in [
            b"bad json".to_vec(),
            serde_json::to_vec(&Entry {
                format: 9,
                key: "bad".into(),
                counted: count(),
            })
            .unwrap(),
            serde_json::to_vec(&Entry {
                format: 1,
                key: "bad".into(),
                counted: Counted {
                    words: vec![],
                    lines: vec![vec![9]],
                },
            })
            .unwrap(),
        ] {
            std::fs::write(store.path("bad"), bytes).unwrap();
            assert!(store.get("bad", &cancel).is_none());
            assert!(!store.path("bad").exists());
        }
        let file = std::fs::File::create(store.path("big")).unwrap();
        file.set_len(ENTRY_BYTES + 1).unwrap();
        assert!(store.get("big", &cancel).is_none());
        let cancel = Cancel::new();
        cancel.cancel();
        store.put("cancelled", &count(), &cancel);
        assert!(!store.path("cancelled").exists());
        assert!(store.get("valid", &cancel).is_none());
        assert!(
            store.path("valid").exists(),
            "cancelled read leaves a complete older cache"
        );
    }

    // T-KNW-025: entry and byte quotas evict least recently used complete entries.
    #[test]
    fn eviction_respects_count_bytes_and_last_access() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store {
            dir: dir.path().into(),
            entries: 2,
            bytes: 32 << 20,
        };
        let cancel = Cancel::new();
        for (i, key) in ["one", "two"].iter().enumerate() {
            store.put(key, &count(), &cancel);
            std::fs::OpenOptions::new()
                .write(true)
                .open(store.path(key))
                .unwrap()
                .set_times(
                    std::fs::FileTimes::new()
                        .set_modified(UNIX_EPOCH + std::time::Duration::from_secs(i as u64 + 1)),
                )
                .unwrap();
        }
        assert!(store.get("one", &cancel).is_some());
        store.put("three", &count(), &cancel);
        assert!(store.get("two", &cancel).is_none());
        assert!(store.get("one", &cancel).is_some());
        let bytes = std::fs::metadata(store.path("one")).unwrap().len();
        let store = Store {
            bytes: bytes - 1,
            ..store
        };
        store.prune();
        assert!(std::fs::read_dir(dir.path()).unwrap().next().is_none());
    }
}
