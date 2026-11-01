// SPDX-License-Identifier: GPL-3.0-only

//! A small EPUB for the tests (feature `test-support`).

use std::io::Write;
use std::path::Path;

use zip::write::SimpleFileOptions;

/// Writes an EPUB of two chapters at `path`.
pub fn write(path: &Path) -> zip::result::ZipResult<()> {
    let mut zip = zip::ZipWriter::new(std::fs::File::create(path)?);
    let options = SimpleFileOptions::default();
    let files = [
        (
            "META-INF/container.xml",
            r#"<container><rootfiles><rootfile full-path="book.opf"/></rootfiles></container>"#,
        ),
        (
            "book.opf",
            r#"<package><metadata><dc:title>見本の本</dc:title><dc:creator>作者</dc:creator></metadata><manifest><item id="a" href="a.xhtml"/><item id="b" href="b.xhtml"/></manifest><spine><itemref idref="a"/><itemref idref="b"/></spine></package>"#,
        ),
        (
            "a.xhtml",
            "<html><body><h1>第一章</h1><p>今日は<ruby>林檎<rt>りんご</rt></ruby>を食べました。とても美味しい。</p><p>「ラーメンが好き」と彼は言った。ABC 123、それから……</p></body></html>",
        ),
        (
            "b.xhtml",
            "<html><body><h1>第二章</h1><p>明日も食べました。</p></body></html>",
        ),
    ];
    for (name, text) in files {
        zip.start_file(name, options)?;
        zip.write_all(text.as_bytes())?;
    }
    zip.finish()?;
    Ok(())
}
