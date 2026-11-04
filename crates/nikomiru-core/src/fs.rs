// SPDX-License-Identifier: GPL-3.0-only

//! File-system helpers.

use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

/// Writes `bytes` to `path` atomically: a temporary file in the same directory,
/// `fsync`, `rename`, then `fsync` of the directory. A crash leaves either the old
/// file or the new one, never a partial file (NFR-REL-01). The new file gets the
/// permissions `mode` (e.g. `0o600` for `config.toml`) on Unix; on Windows it
/// takes those of its folder, and the folder is not synced, since Windows
/// cannot open a folder as a file.
pub fn write_atomic(path: &Path, bytes: &[u8], mode: u32) -> std::io::Result<()> {
    let dir = path
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir)?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = dir.join(format!(".{name}.tmp-{}", std::process::id()));
    let result = (|| {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        options.mode(mode);
        #[cfg(not(unix))]
        let _ = mode;
        let mut f = options.open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        std::fs::rename(&tmp, path)?;
        #[cfg(unix)]
        std::fs::File::open(dir)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// `path` resolved, as `std::fs::canonicalize` does, but without the `\\?\`
/// that Windows puts before a path that does not need it, so that the path
/// reads as the user knows it and compares with the others (ADR 0009).
pub fn canonical(path: &Path) -> std::io::Result<std::path::PathBuf> {
    let resolved = std::fs::canonicalize(path)?;
    if cfg!(windows)
        && let Some(plain) = resolved.to_str().and_then(without_verbatim)
    {
        return Ok(plain.into());
    }
    Ok(resolved)
}

/// A Windows verbatim path as an ordinary one, when it can be one: a drive
/// path or a share, short enough for the usual limit of 260 characters, with
/// no name Windows would read otherwise (a trailing dot or space).
fn without_verbatim(path: &str) -> Option<String> {
    let plain = if let Some(share) = path.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{share}")
    } else {
        let rest = path.strip_prefix(r"\\?\")?;
        let drive = rest.as_bytes();
        if drive.len() < 3 || !drive[0].is_ascii_alphabetic() || &drive[1..3] != br":\" {
            return None;
        }
        rest.to_string()
    };
    let odd = plain
        .split('\\')
        .any(|name| name.ends_with('.') && name != "." && name != ".." || name.ends_with(' '));
    (plain.encode_utf16().count() < 260 && !odd).then_some(plain)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    // ADR 0009: the verbatim paths of Windows, made ordinary when they can be
    #[test]
    fn verbatim_paths_become_ordinary() {
        assert_eq!(
            without_verbatim(r"\\?\C:\Users\u\Videos").as_deref(),
            Some(r"C:\Users\u\Videos")
        );
        assert_eq!(
            without_verbatim(r"\\?\UNC\server\share\x.mkv").as_deref(),
            Some(r"\\server\share\x.mkv")
        );
        assert_eq!(without_verbatim(r"C:\plain"), None, "nothing to remove");
        assert_eq!(without_verbatim(r"\\?\Volume{1234}\x"), None, "no drive");
        assert_eq!(without_verbatim(r"\\?\C:\a\name."), None, "trailing dot");
        let long = format!(r"\\?\C:\{}", "x".repeat(300));
        assert_eq!(without_verbatim(&long), None, "too long");
    }

    #[test]
    fn replaces_content_and_sets_permissions() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub/config.toml");
        write_atomic(&path, b"a = 1\n", 0o600).unwrap();
        write_atomic(&path, b"a = 2\n", 0o600).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "a = 2\n");
        #[cfg(unix)]
        {
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
        // no temporary files left behind
        let names: Vec<_> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, ["config.toml"]);
    }

    #[test]
    fn failure_keeps_the_old_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        write_atomic(&path, b"old", 0o600).unwrap();
        // the target is a directory: rename fails, the temporary file is removed
        let target = dir.path().join("as-dir");
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("x"), b"").unwrap();
        assert!(write_atomic(&target, b"new", 0o600).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"old");
        assert!(
            std::fs::read_dir(dir.path()).unwrap().all(|e| !e
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains(".tmp-"))
        );
    }
}
