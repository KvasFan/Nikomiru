// SPDX-License-Identifier: GPL-3.0-only

//! The dictionary zips of TEST §4.3 that an import must refuse (`evil-*`) or
//! accept although they are unusual (`wrapped-dir`, `unicode-path`, `zip64`).
//! Some need bytes no zip library writes (a `../` name, a declared size that
//! lies, an Info-ZIP Unicode Path field), hence the small raw writer.

use std::io::Write;
use std::path::Path;

use anyhow::Result;

use crate::fixtures::{fresh, stamp, zip_options};

/// An entry of a raw zip: stored, with control over every field.
struct Raw {
    name: Vec<u8>,
    data: Vec<u8>,
    /// Uncompressed size written in the headers, if it must lie.
    declared: Option<u32>,
    extra: Vec<u8>,
    /// Unix mode in the external attributes (symlinks).
    unix_mode: Option<u32>,
}

impl Raw {
    fn file(name: &str, data: &[u8]) -> Self {
        Self {
            name: name.as_bytes().to_vec(),
            data: data.to_vec(),
            declared: None,
            extra: Vec::new(),
            unix_mode: None,
        }
    }
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = flate2::Crc::new();
    crc.update(data);
    crc.sum()
}

/// A zip of stored entries, written field by field (APPNOTE 4.3).
fn raw_zip(entries: &[Raw]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for e in entries {
        let offset = out.len() as u32;
        let crc = crc32(&e.data);
        let size = e.data.len() as u32;
        let declared = e.declared.unwrap_or(size);
        let le16 = |v: u16| v.to_le_bytes();
        let le32 = |v: u32| v.to_le_bytes();
        // local file header
        out.extend_from_slice(&le32(0x0403_4b50));
        out.extend_from_slice(&le16(20));
        out.extend_from_slice(&le16(0)); // flags: no UTF-8 bit
        out.extend_from_slice(&le16(0)); // stored
        out.extend_from_slice(&le16(0));
        out.extend_from_slice(&le16(0x21));
        out.extend_from_slice(&le32(crc));
        out.extend_from_slice(&le32(size));
        out.extend_from_slice(&le32(declared));
        out.extend_from_slice(&le16(e.name.len() as u16));
        out.extend_from_slice(&le16(e.extra.len() as u16));
        out.extend_from_slice(&e.name);
        out.extend_from_slice(&e.extra);
        out.extend_from_slice(&e.data);
        // central directory header
        let made_by: u16 = if e.unix_mode.is_some() {
            (3 << 8) | 20
        } else {
            20
        };
        central.extend_from_slice(&le32(0x0201_4b50));
        central.extend_from_slice(&le16(made_by));
        central.extend_from_slice(&le16(20));
        central.extend_from_slice(&le16(0));
        central.extend_from_slice(&le16(0));
        central.extend_from_slice(&le16(0));
        central.extend_from_slice(&le16(0x21));
        central.extend_from_slice(&le32(crc));
        central.extend_from_slice(&le32(size));
        central.extend_from_slice(&le32(declared));
        central.extend_from_slice(&le16(e.name.len() as u16));
        central.extend_from_slice(&le16(e.extra.len() as u16));
        central.extend_from_slice(&le16(0)); // comment
        central.extend_from_slice(&le16(0)); // disk
        central.extend_from_slice(&le16(0)); // internal attributes
        central.extend_from_slice(&le32(e.unix_mode.map_or(0, |m| m << 16)));
        central.extend_from_slice(&le32(offset));
        central.extend_from_slice(&e.name);
        central.extend_from_slice(&e.extra);
    }
    let cd_offset = out.len() as u32;
    let cd_size = central.len() as u32;
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&[0, 0, 0, 0]);
    let n = (entries.len() as u16).to_le_bytes();
    out.extend_from_slice(&n);
    out.extend_from_slice(&n);
    out.extend_from_slice(&cd_size.to_le_bytes());
    out.extend_from_slice(&cd_offset.to_le_bytes());
    out.extend_from_slice(&[0, 0]);
    out
}

const INDEX: &str = r#"{"title":"Nikomiru evil","revision":"1","format":3}"#;
const TERMS: &str = r#"[["猫","ねこ","n","",0,["cat"],1,""]]"#;

/// Writes the zips into `out` (`target/fixtures/dicts`), from `mini_ja` (the
/// source folder of `mini-ja`).
pub fn generate(out: &Path, mini_ja: &Path) -> Result<()> {
    std::fs::create_dir_all(out)?;
    let save = |name: &str, recipe: &str, make: &dyn Fn() -> Result<Vec<u8>>| -> Result<()> {
        let target = out.join(name);
        if fresh(&target, recipe) {
            return Ok(());
        }
        std::fs::write(&target, make()?)?;
        stamp(&target, recipe)
    };
    let basic = |extra: Raw| {
        raw_zip(&[
            Raw::file("index.json", INDEX.as_bytes()),
            Raw::file("term_bank_1.json", TERMS.as_bytes()),
            extra,
        ])
    };
    save("evil-traversal.zip", "raw v1", &|| {
        Ok(basic(Raw::file("../x", b"escape")))
    })?;
    save("evil-absolute.zip", "raw v1", &|| {
        Ok(basic(Raw::file("/etc/x", b"escape")))
    })?;
    save("evil-symlink.zip", "raw v1", &|| {
        Ok(basic(Raw {
            unix_mode: Some(0o120_777),
            ..Raw::file("link", b"/etc/passwd")
        }))
    })?;
    save("evil-huge-entry.zip", "raw v1", &|| {
        Ok(basic(Raw {
            declared: Some(600 << 20),
            ..Raw::file("img/huge.png", b"tiny")
        }))
    })?;
    save("evil-badjson.zip", "raw v1", &|| {
        Ok(raw_zip(&[
            Raw::file("index.json", INDEX.as_bytes()),
            Raw::file("term_bank_1.json", b"[[\"broken"),
        ]))
    })?;
    // the name is not UTF-8 in the header; the 0x7075 field carries 字
    save("unicode-path.zip", "raw v1", &|| {
        let header_name = b"img/\x8e\x9a.png".to_vec();
        let unicode = "img/字.png";
        let mut extra = Vec::new();
        let body: Vec<u8> = [
            &[1u8][..],
            &crc32(&header_name).to_le_bytes(),
            unicode.as_bytes(),
        ]
        .concat();
        extra.extend_from_slice(&0x7075u16.to_le_bytes());
        extra.extend_from_slice(&(body.len() as u16).to_le_bytes());
        extra.extend_from_slice(&body);
        Ok(basic(Raw {
            name: header_name,
            extra,
            ..Raw::file("", b"\x89PNG")
        }))
    })?;
    // 100 MiB of zeros deflate to ~100 KiB: a ratio far above 200:1
    save("evil-bomb.zip", "deflate 100MiB v1", &|| {
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut buf);
            zip.start_file("index.json", zip_options(6))?;
            zip.write_all(INDEX.as_bytes())?;
            zip.start_file("term_bank_1.json", zip_options(9))?;
            let zeros = vec![b' '; 1 << 20];
            for _ in 0..100 {
                zip.write_all(&zeros)?;
            }
            zip.finish()?;
        }
        Ok(buf.into_inner())
    })?;
    // mini-ja inside one folder, with macOS litter: accepted (BUG-2053)
    let files = {
        let mut f = Vec::new();
        collect(mini_ja, &mut f)?;
        f.sort();
        f
    };
    let wrapped = |prefix: &str, large: bool| -> Result<Vec<u8>> {
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut buf);
            for f in &files {
                let inner = f
                    .strip_prefix(mini_ja)?
                    .to_string_lossy()
                    .replace('\\', "/");
                zip.start_file(format!("{prefix}{inner}"), zip_options(6).large_file(large))?;
                zip.write_all(&std::fs::read(f)?)?;
            }
            if !prefix.is_empty() {
                zip.start_file(format!("{prefix}.DS_Store"), zip_options(6))?;
                zip.write_all(b"macOS")?;
                zip.start_file("__MACOSX/mini-ja/._index.json", zip_options(6))?;
                zip.write_all(b"macOS")?;
            }
            zip.finish()?;
        }
        Ok(buf.into_inner())
    };
    let recipe = format!("v1 {}", files.len());
    save("wrapped-dir.zip", &recipe, &|| wrapped("mini-ja/", false))?;
    save("zip64.zip", &recipe, &|| wrapped("", true))?;
    Ok(())
}

fn collect(dir: &Path, out: &mut Vec<std::path::PathBuf>) -> Result<()> {
    for e in std::fs::read_dir(dir)? {
        let p = e?.path();
        if p.is_dir() {
            collect(&p, out)?;
        } else {
            out.push(p);
        }
    }
    Ok(())
}
