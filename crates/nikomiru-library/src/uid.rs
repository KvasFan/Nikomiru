// SPDX-License-Identifier: GPL-3.0-only

//! Identity of a video (DAT-003): `"video/" + sanitize(file name without
//! extension)`. Two different files with the same name get `" (2)"`, `" (3)"`…

use std::path::Path;

/// Longest sanitized name, in characters.
const MAX_LEN: usize = 200;

/// The file name without extension, cleaned up: path separators, characters that
/// are invalid in file names on common systems and control characters become `_`,
/// runs of whitespace become one space, and the result is trimmed and limited to
/// 200 characters.
pub fn sanitize(stem: &str) -> String {
    let mut out = String::with_capacity(stem.len());
    let mut space = false;
    for c in stem.chars() {
        if c.is_whitespace() {
            space = !out.is_empty();
            continue;
        }
        if space {
            out.push(' ');
            space = false;
        }
        let c = if c.is_control()
            || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
        {
            '_'
        } else {
            c
        };
        out.push(c);
    }
    out.chars().take(MAX_LEN).collect()
}

/// The base uid of a file (without the `" (n)"` suffix).
pub fn base_uid(path: &Path) -> String {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    format!("video/{}", sanitize(&stem))
}

/// The uid for `path`, given a function that tells which path (if any) already
/// owns a uid: the base uid, or the first free `" (n)"` variant.
pub fn unique_uid(path: &Path, owner: impl Fn(&str) -> Option<String>) -> String {
    let base = base_uid(path);
    let mine = path.to_string_lossy();
    let free_or_mine = |uid: &str| owner(uid).is_none_or(|p| p == mine);
    if free_or_mine(&base) {
        return base;
    }
    (2..)
        .map(|n| format!("{base} ({n})"))
        .find(|uid| free_or_mine(uid))
        .unwrap_or(base)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    // T-DAT-003
    #[test]
    fn sanitize_is_stable() {
        assert_eq!(
            sanitize("[Sub] Show - 01 (1080p)"),
            "[Sub] Show - 01 (1080p)"
        );
        assert_eq!(sanitize("  a\tb\n\nc  "), "a b c");
        assert_eq!(
            sanitize("a/b\\c:d*e?f\"g<h>i|j\u{7}"),
            "a_b_c_d_e_f_g_h_i_j_"
        );
        assert_eq!(sanitize("日本語 エピソード"), "日本語 エピソード");
        assert_eq!(sanitize(&"x".repeat(500)).len(), 200);
        assert_eq!(
            sanitize(&sanitize("  odd / name ")),
            sanitize("  odd / name ")
        );
        assert_eq!(base_uid(Path::new("/v/Ep 01.mkv")), "video/Ep 01");
    }

    // T-DAT-003
    #[test]
    fn same_name_in_another_folder_gets_a_suffix() {
        let mut owners: HashMap<String, String> = HashMap::new();
        owners.insert("video/Ep01".into(), "/a/Ep01.mkv".into());
        owners.insert("video/Ep01 (2)".into(), "/b/Ep01.mkv".into());
        let owner = |uid: &str| owners.get(uid).cloned();
        assert_eq!(unique_uid(Path::new("/a/Ep01.mkv"), owner), "video/Ep01");
        assert_eq!(
            unique_uid(Path::new("/b/Ep01.mkv"), owner),
            "video/Ep01 (2)"
        );
        assert_eq!(
            unique_uid(Path::new("/c/Ep01.mp4"), owner),
            "video/Ep01 (3)"
        );
    }
}
