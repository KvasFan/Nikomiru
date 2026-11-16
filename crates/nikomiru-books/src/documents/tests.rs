// SPDX-License-Identifier: GPL-3.0-only

//! Real format fixtures and malformed/untrusted document boundaries.
use super::*;
use std::io::Write;
fn archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut z = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        z.start_file(
            *name,
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated),
        )
        .unwrap();
        z.write_all(bytes).unwrap();
    }
    z.finish().unwrap().into_inner()
}
fn texts(chapters: &[Chapter]) -> Vec<&str> {
    chapters
        .iter()
        .flat_map(|c| &c.paragraphs)
        .map(|p| p.text.as_str())
        .collect()
}
fn rtf_text(bytes: &[u8], support: &dyn Support) -> Vec<String> {
    rtf::read(bytes, support, &Cancel::new())
        .unwrap()
        .iter()
        .flat_map(|c| &c.paragraphs)
        .map(|p| p.text.clone())
        .collect()
}
const TYPES: &str = r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#;
fn docx_file(xml: &str) -> Vec<u8> {
    archive(&[("[Content_Types].xml",TYPES.as_bytes()),("word/document.xml",xml.as_bytes()),("word/_rels/document.xml.rels",br#"<Relationships><Relationship Target="https://example.invalid/never-fetch"/></Relationships>"#)])
}
// T-DOC-001: archive order never determines the selected main document; executable contents stay inert.
#[test]
fn htmlz_selects_a_safe_main_document_and_preserves_ruby() {
    let bytes=archive(&[("ignored.html",b"<p>wrong</p>"),("index.html","<html><script>bad()</script><style>secret</style><h1>Title</h1><p><ruby>猫<rt>ねこ</rt></ruby>です。<a href='https://example.invalid/never-fetch'>次</a></p><img src='remote'/></html>".as_bytes())]);
    let c = htmlz(&bytes, &Cancel::new()).unwrap();
    assert_eq!(texts(&c), ["Title", "猫です。次"]);
    assert_eq!(c[0].paragraphs[1].ruby[0].text, "ねこ");
    assert!(
        htmlz(
            &archive(&[("one.html", b"<p>one</p>"), ("two.html", b"<p>two</p>")]),
            &Cancel::new()
        )
        .is_err()
    );
    for name in [
        "../index.html",
        "a/../index.html",
        "/index.html",
        "C:/index.html",
        "a\\index.html",
    ] {
        assert!(
            htmlz(&archive(&[(name, b"<p>no</p>")]), &Cancel::new()).is_err(),
            "{name}"
        );
    }
    assert!(
        htmlz(
            &archive(&[("index.html", b"<p>one</p>"), ("INDEX.HTML", b"<p>two</p>")]),
            &Cancel::new()
        )
        .is_err()
    );
    let huge = vec![b'x'; MAX_ENTRY + 1];
    assert!(matches!(
        htmlz(&archive(&[("index.html", &huge)]), &Cancel::new()),
        Err(BookError::TooLarge)
    ));
}
// T-DOC-003: standard/strict namespaces, real run/table/paragraph order and literal entity handling.
#[test]
fn docx_retains_body_order_and_ignores_deleted_fields_drawings_and_external_parts() {
    for namespace in [
        "http://schemas.openxmlformats.org/wordprocessingml/2006/main",
        "http://purl.oclc.org/ooxml/wordprocessingml/main",
    ] {
        let xml = format!(
            r#"<?xml version="1.0"?><w:document xmlns:w="{namespace}"><w:body><w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>猫&#x3067;&#12377; &amp; </w:t></w:r><w:del><w:r><w:delText>deleted</w:delText></w:r></w:del><w:ins><w:r><w:t>new</w:t><w:tab/><w:t>text</w:t><w:br/><w:t><![CDATA[<literal>]]></w:t></w:r></w:ins><w:r><w:instrText>INCLUDEPICTURE https://example.invalid</w:instrText><w:t>result</w:t><w:drawing><w:p><w:r><w:t>ignored</w:t></w:r></w:p></w:drawing></w:r></w:p><w:tbl><w:tr><w:tc><w:p><w:r><w:t>left</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>right</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p/></w:body></w:document>"#
        );
        let c = docx::read(&docx_file(&xml), &Cancel::new()).unwrap();
        assert_eq!(
            texts(&c),
            ["猫です & new\ttext\n<literal>result", "left", "right", ""]
        );
        assert!(c[0].paragraphs[0].heading);
    }
    for xml in [
        r#"<!DOCTYPE w:document [<!ENTITY x SYSTEM "file:///never">]><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>&x;</w:t></w:r></w:p></w:body></w:document>"#,
        r#"<w:document><w:body><w:p><w:r><w:t>undeclared</w:t></w:r></w:p></w:body></w:document>"#,
        r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>unclosed"#,
    ] {
        assert!(docx::read(&docx_file(xml), &Cancel::new()).is_err());
    }
    let xml = format!(
        r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{}<w:p><w:r><w:t>deep</w:t></w:r></w:p>{}</w:body></w:document>"#,
        "<w:sdt>".repeat(65),
        "</w:sdt>".repeat(65)
    );
    assert!(matches!(
        docx::read(&docx_file(&xml), &Cancel::new()),
        Err(BookError::TooLarge)
    ));
}

// T-DOC-003: malformed metadata/attributes cannot masquerade as a valid body part.
#[test]
fn docx_rejects_invalid_namespaces_attributes_and_content_outside_the_root() {
    let valid = r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>猫</w:t></w:r></w:p></w:body></w:document>"#;
    for xml in [
        format!("<![CDATA[unexpected]]>{valid}"),
        format!("{valid}<![CDATA[unexpected]]>"),
        valid.replace("<w:r>", "<w:r bad='1' bad='2'>"),
        valid.replace("<w:r>", "<w:r unknown:attr='value'>"),
        valid.replace("<w:r>", "<w:r attr='&undefined;'>"),
        valid.replace("<w:r>", "<w:r attr='unclosed>"),
    ] {
        assert!(
            docx::read(&docx_file(&xml), &Cancel::new()).is_err(),
            "{xml}"
        );
    }
    for types in [
        TYPES.replace("<Override", "<Override xmlns='urn:unrelated'"),
        TYPES
            .replace("<Override", "<nested><Override")
            .replace("/></Types>", "/></nested></Types>"),
        format!("{TYPES}<![CDATA[unexpected]]>"),
    ] {
        let bytes = archive(&[
            ("[Content_Types].xml", types.as_bytes()),
            ("word/document.xml", valid.as_bytes()),
        ]);
        assert!(docx::read(&bytes, &Cancel::new()).is_err(), "{types}");
    }
}
// T-DOC-002: scoped fallback counts, signed UTF-16 units, escaped syntax and ignored destinations.
#[test]
fn rtf_preserves_unicode_surrogates_literal_escapes_and_visible_field_results() {
    assert_eq!(
        rtf_text(
            br"{\rtf1\ansi\uc1 \u29483?\u12391?\u12377?\par \u-10179?\u-8704?\tab next\line line}",
            &Builtin
        ),
        ["猫です", "😀\tnext\nline"]
    );
    assert_eq!(
        rtf_text(
            br"{\rtf1 \{literal\} \\ \u233\{x {\uc0\u945}\u233?}",
            &Builtin
        ),
        ["{literal} \\ éx αé"]
    );
    assert_eq!(rtf_text(br#"{\rtf1 A{\info metadata}{\*\unknown ignored}{\pict\bin4 {\}?}{\field{\*\fldinst INCLUDEPICTURE "https://example.invalid"}{\fldrslt result}}B}"#,&Builtin),["AresultB"]);
    assert_eq!(
        rtf_text(br"{\rtf1 {\upr{ANSI}{\*\ud \u29483?}}}", &Builtin),
        ["猫"]
    );
    assert_eq!(
        rtf_text(
            br"{\rtf1 A{\pict{\upr{bad}{\*\ud must-stay-ignored}}}B}",
            &Builtin
        ),
        ["AB"]
    );
}
struct Legacy;
impl Support for Legacy {
    fn decode(&self, b: &[u8], encoding: &str) -> Option<String> {
        if encoding == "CP932" && b == [0x82, 0xa0] {
            Some("あ".into())
        } else {
            Builtin.decode(b, encoding)
        }
    }
    fn pdf_text(&self, _: &[u8], _: &Cancel) -> Result<String, BookError> {
        Err(BookError::PdfUnavailable)
    }
}
// T-DOC-002: consecutive hex bytes remain a single multibyte run; font selection and group restoration are exact.
#[test]
fn rtf_decodes_declared_legacy_runs_and_scoped_font_code_pages() {
    let bytes=br"{\rtf1\ansi\ansicpg1252{\fonttbl{\f0\fcharset0 Arial;}{\f1\fcharset128 MS;}}\f0 caf\'e9 {\f1\'82\'a0} caf\'e9}";
    assert_eq!(rtf_text(bytes, &Legacy), ["café あ café"]);
    assert!(rtf::read(bytes, &Builtin, &Cancel::new()).is_err());
    assert_eq!(
        rtf_text(b"{\\rtf1\\ansi\\ansicpg65001 \xe7\x8c\xab}", &Builtin),
        ["猫"]
    );
}
// T-DOC-002: no partial reading from malformed Unicode/control/binary/group structures.
#[test]
fn rtf_rejects_malformed_input_and_enforces_nesting_and_text_limits() {
    for text in [
        "text",
        r"{\rtf12 text}",
        r"{\rtf1 unclosed",
        r"{\rtf1}trailing",
        r"{\rtf1\uc-1 text}",
        r"{\rtf1\uc33 text}",
        r"{\rtf1\u-10179?}",
        r"{\rtf1\u-8704?}",
        r"{\rtf1\'gg}",
        r"{\rtf1\bin99999999999999999 x}",
        r"{\rtf1\bin20 x}",
    ] {
        assert!(
            rtf::read(text.as_bytes(), &Builtin, &Cancel::new()).is_err(),
            "{text}"
        );
    }
    let deep = format!("{{\\rtf1 {}text{}}}", "{".repeat(64), "}".repeat(64));
    assert!(matches!(
        rtf::read(deep.as_bytes(), &Builtin, &Cancel::new()),
        Err(BookError::TooLarge)
    ));
    let long = format!(
        "{{\\rtf1 {}}}",
        "x".repeat(crate::text::MAX_PARAGRAPH_BYTES + 1)
    );
    assert!(matches!(
        rtf::read(long.as_bytes(), &Builtin, &Cancel::new()),
        Err(BookError::TooLarge)
    ));
}
struct Pdf;
impl Support for Pdf {
    fn decode(&self, b: &[u8], e: &str) -> Option<String> {
        Builtin.decode(b, e)
    }
    fn pdf_text(&self, _: &[u8], _: &Cancel) -> Result<String, BookError> {
        Ok("猫です。\n\u{c}\u{c}Third page\n\u{c}".into())
    }
}
// T-DOC-004, T-DOC-005: page indices, exact source identities, changed content and cancellation are shared with normal reader locations.
#[test]
fn imported_documents_keep_content_identity_and_original_pdf_page_indices() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Owned.pdf");
    std::fs::write(&path, b"%PDF-1.7 frozen bytes").unwrap();
    let book = read(&path, &Pdf, &Cancel::new()).unwrap();
    assert_eq!(book.chapters.len(), 3);
    assert!(book.chapters[1].paragraphs.is_empty());
    assert_eq!(book.chapters[2].paragraphs[0].text, "Third page");
    let other = dir.path().join("moved/Owned.pdf");
    std::fs::create_dir(other.parent().unwrap()).unwrap();
    std::fs::copy(&path, &other).unwrap();
    assert_eq!(
        book.identity,
        read(&other, &Pdf, &Cancel::new()).unwrap().identity
    );
    std::fs::write(&path, b"%PDF-1.7 changed bytes").unwrap();
    assert_ne!(
        book.identity,
        read(&path, &Pdf, &Cancel::new()).unwrap().identity
    );
    assert!(matches!(
        read(&path, &Builtin, &Cancel::new()),
        Err(BookError::PdfUnavailable)
    ));
    let cancel = Cancel::new();
    cancel.cancel();
    assert!(read(&path, &Pdf, &cancel).is_err());
}

struct Extracted(String);
impl Support for Extracted {
    fn decode(&self, b: &[u8], e: &str) -> Option<String> {
        Builtin.decode(b, e)
    }
    fn pdf_text(&self, _: &[u8], _: &Cancel) -> Result<String, BookError> {
        Ok(self.0.clone())
    }
}

// T-DOC-001, T-DOC-003, T-DOC-004, T-DOC-005: input and converted-output quotas
// are independent; empty source paragraphs/pages still retain bounded indices.
#[test]
fn documents_enforce_archive_source_and_converted_text_budgets() {
    let cancel = Cancel::new();
    let entries: Vec<_> = (0..=MAX_ENTRIES)
        .map(|i| (format!("{i}.html"), Vec::<u8>::new()))
        .collect();
    let refs: Vec<_> = entries
        .iter()
        .map(|(name, b)| (name.as_str(), b.as_slice()))
        .collect();
    assert!(matches!(
        htmlz(&archive(&refs), &cancel),
        Err(BookError::TooLarge)
    ));
    let large = vec![b'x'; MAX_ENTRY];
    let entries: Vec<_> = (0..17)
        .map(|i| (format!("{i}.html"), large.as_slice()))
        .collect();
    let refs: Vec<_> = entries
        .iter()
        .map(|(name, b)| (name.as_str(), *b))
        .collect();
    assert!(matches!(
        htmlz(&archive(&refs), &cancel),
        Err(BookError::TooLarge)
    ));
    let xml = format!(
        r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{}</w:body></w:document>"#,
        "<w:p/>".repeat(crate::text::MAX_PARAGRAPHS + 1)
    );
    assert!(matches!(
        docx::read(&docx_file(&xml), &cancel),
        Err(BookError::TooLarge)
    ));
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Bounded.pdf");
    std::fs::write(&path, b"%PDF-1.7 original").unwrap();
    for text in [
        format!("{}\u{c}", "page\u{c}".repeat(512)),
        "x".repeat(crate::text::MAX_PARAGRAPH_BYTES + 1),
        "x\n".repeat(crate::text::MAX_PARAGRAPHS + 1),
        "x".repeat(MAX_ENTRY + 1),
    ] {
        assert!(matches!(
            read(&path, &Extracted(text), &cancel),
            Err(BookError::TooLarge)
        ));
    }
    let before = read(&path, &Extracted("same source".into()), &cancel).unwrap();
    let changed = read(&path, &Extracted("other extraction".into()), &cancel).unwrap();
    assert_ne!(
        before.identity, changed.identity,
        "helper output participates in identity"
    );
    assert!(matches!(
        read(&path, &Extracted("\u{c}".into()), &cancel),
        Err(BookError::NoText)
    ));
    for (name, max) in [("Large.pdf", MAX_ARCHIVE), ("Large.rtf", MAX_ENTRY)] {
        let path = dir.path().join(name);
        std::fs::File::create(&path)
            .unwrap()
            .set_len(max as u64 + 1)
            .unwrap();
        assert!(matches!(
            read(&path, &Builtin, &cancel),
            Err(BookError::TooLarge)
        ));
    }
}
