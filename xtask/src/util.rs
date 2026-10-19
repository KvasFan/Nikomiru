// SPDX-License-Identifier: GPL-3.0-only

//! Helpers shared by the tasks: paths, processes, file walking, PRNG.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

/// Workspace root (the directory that contains `xtask/`).
pub fn root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .map_or_else(|| manifest.to_path_buf(), Path::to_path_buf)
}

/// `cargo` (the same one that started xtask), with the root as working directory.
pub fn cargo() -> Command {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
    let mut cmd = Command::new(cargo);
    cmd.current_dir(root());
    cmd
}

/// Runs a command, printing its command line; fails if the exit status is not 0.
pub fn run(cmd: &mut Command) -> Result<()> {
    eprintln!("$ {}", display(cmd));
    let status = cmd
        .status()
        .with_context(|| format!("cannot start `{}`", display(cmd)))?;
    if !status.success() {
        bail!("`{}` failed ({status})", display(cmd));
    }
    Ok(())
}

/// Runs a command and returns its stdout; fails if the exit status is not 0.
pub fn output(cmd: &mut Command) -> Result<String> {
    let out = cmd
        .output()
        .with_context(|| format!("cannot start `{}`", display(cmd)))?;
    if !out.status.success() {
        bail!(
            "`{}` failed ({}):\n{}",
            display(cmd),
            out.status,
            String::from_utf8_lossy(&out.stderr)
        );
    }
    String::from_utf8(out.stdout)
        .with_context(|| format!("`{}`: stdout is not UTF-8", display(cmd)))
}

/// The command line in readable form (for messages only).
pub fn display(cmd: &Command) -> String {
    let mut parts = vec![cmd.get_program().to_string_lossy().into_owned()];
    for arg in cmd.get_args() {
        let arg = arg.to_string_lossy();
        if arg.is_empty() || arg.contains([' ', '\'', '"']) {
            parts.push(format!("'{arg}'"));
        } else {
            parts.push(arg.into_owned());
        }
    }
    parts.join(" ")
}

/// True if `name` is an executable reachable from `PATH`.
pub fn have_program(name: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|dir| dir.join(name).is_file()))
}

/// Collects, in order, the files with extension `ext` under `dir`, skipping `target`
/// and hidden directories.
pub fn walk(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) -> Result<()> {
    let mut entries = std::fs::read_dir(dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if entry.file_type()?.is_dir() {
            if name != "target" && !name.starts_with('.') {
                walk(&path, ext, out)?;
            }
        } else if path.extension().is_some_and(|e| e == ext) {
            out.push(path);
        }
    }
    Ok(())
}

/// Path relative to the root, for messages.
pub fn rel(path: &Path) -> String {
    let root = root();
    path.strip_prefix(&root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

/// Splits a line of Rust code into (code, comment). String contents are removed
/// from the code (the quotes remain), so neither `//` nor keywords inside a string
/// count. `in_block` carries the state of `/* … */` comments from one line to the next.
/// It is an approximate lexer, good enough for the xtask checks.
pub fn split_comment(line: &str, in_block: &mut bool) -> (String, String) {
    let mut code = String::new();
    let mut comment = String::new();
    let mut chars = line.chars().peekable();
    let mut in_str = false;
    while let Some(c) = chars.next() {
        if *in_block {
            comment.push(c);
            if c == '*' && chars.peek() == Some(&'/') {
                comment.push('/');
                chars.next();
                *in_block = false;
            }
            continue;
        }
        if in_str {
            if c == '\\' {
                chars.next();
            } else if c == '"' {
                code.push(c);
                in_str = false;
            }
            continue;
        }
        match c {
            '"' => {
                in_str = true;
                code.push(c);
            }
            '\'' => {
                // Character literal ('"', '\'', 'x'): copied whole, so that a '"'
                // does not open a string. A lifetime ('a) is left as is.
                code.push(c);
                let ahead: Vec<char> = chars.clone().take(3).collect();
                let len = match ahead.as_slice() {
                    ['\\', _, '\'', ..] => 3,
                    [x, '\'', ..] if *x != '\\' => 2,
                    _ => 0,
                };
                for _ in 0..len {
                    if let Some(n) = chars.next() {
                        code.push(n);
                    }
                }
            }
            '/' if chars.peek() == Some(&'/') => {
                comment.push('/');
                comment.extend(chars.by_ref());
                break;
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                comment.push_str("/*");
                *in_block = true;
            }
            _ => code.push(c),
        }
    }
    (code, comment)
}

/// SplitMix64 PRNG: deterministic, dependency-free, for the generated fixtures.
#[derive(Debug, Clone)]
pub struct SplitMix64(u64);

impl SplitMix64 {
    /// New generator with the given seed.
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    /// Next 64-bit value.
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform integer in `0..n` (`n > 0`).
    pub fn below(&mut self, n: usize) -> usize {
        // The bias is negligible for n ≪ 2^64.
        (self.next_u64() % n.max(1) as u64) as usize
    }

    /// Uniform integer in `lo..=hi`.
    pub fn range(&mut self, lo: usize, hi: usize) -> usize {
        lo + self.below(hi - lo + 1)
    }

    /// True with probability `p`.
    pub fn chance(&mut self, p: f64) -> bool {
        ((self.next_u64() >> 11) as f64 / (1u64 << 53) as f64) < p
    }

    /// A random element of the (non-empty) slice.
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_comment_ignores_slashes_in_strings() {
        let mut block = false;
        let (code, comment) = split_comment(r#"let s = "// not a comment"; // real"#, &mut block);
        assert_eq!(code.trim(), r#"let s = "";"#);
        assert_eq!(comment, "// real");
    }

    #[test]
    fn split_comment_handles_char_literals_and_blocks() {
        let mut block = false;
        let (code, comment) = split_comment(r#"let q = '"'; /* a"#, &mut block);
        assert_eq!(code.trim(), r#"let q = '"';"#);
        assert_eq!(comment, "/* a");
        assert!(block);
        let (code, comment) = split_comment("b */ x // c", &mut block);
        assert_eq!(comment, "b */// c");
        assert_eq!(code.trim(), "x");
        assert!(!block);
    }

    #[test]
    fn split_comment_keeps_lifetimes() {
        let mut block = false;
        let (code, comment) = split_comment("fn f<'a>(x: &'a str) {} // ok", &mut block);
        assert_eq!(code.trim(), "fn f<'a>(x: &'a str) {}");
        assert_eq!(comment, "// ok");
    }

    #[test]
    fn splitmix_is_deterministic() {
        let mut a = SplitMix64::new(42);
        let mut b = SplitMix64::new(42);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
        assert!((0..1000).all(|_| a.below(7) < 7));
    }
}
