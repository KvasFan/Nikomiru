// SPDX-License-Identifier: GPL-3.0-only

//! Namespace-aware inert WordprocessingML body text, preserving source paragraph order.
use super::*;
use crate::Paragraph;
use quick_xml::{
    NsReader,
    events::{BytesStart, Event},
    name::ResolveResult,
};
fn attributes(r: &NsReader<&[u8]>, e: &BytesStart<'_>) -> Result<(), BookError> {
    let mut names = HashSet::new();
    for a in e.attributes() {
        let a = a.map_err(|_| bad())?;
        a.normalized_value(quick_xml::XmlVersion::Implicit1_0)
            .map_err(|_| bad())?;
        if a.key.as_ref() == b"xmlns" || a.key.as_ref().starts_with(b"xmlns:") {
            continue;
        }
        let (ns, local) = r.resolver().resolve_attribute(a.key);
        let ns = match ns {
            ResolveResult::Bound(n) => n.as_ref().to_vec(),
            ResolveResult::Unbound => Vec::new(),
            ResolveResult::Unknown(_) => return Err(bad()),
        };
        if !names.insert((ns, local.as_ref().to_vec())) {
            return Err(bad());
        }
    }
    Ok(())
}
fn word(ns: &ResolveResult<'_>) -> bool {
    matches!(ns,ResolveResult::Bound(n) if matches!(n.as_ref(),b"http://schemas.openxmlformats.org/wordprocessingml/2006/main"|b"http://purl.oclc.org/ooxml/wordprocessingml/main"))
}
fn reference(bytes: &[u8]) -> Result<char, BookError> {
    let s = std::str::from_utf8(bytes).map_err(|_| bad())?;
    match s {
        "amp" => Ok('&'),
        "lt" => Ok('<'),
        "gt" => Ok('>'),
        "apos" => Ok('\''),
        "quot" => Ok('"'),
        _ => {
            let value = if let Some(hex) = s.strip_prefix("#x") {
                u32::from_str_radix(hex, 16).ok()
            } else {
                s.strip_prefix('#').and_then(|d| d.parse().ok())
            };
            value
                .and_then(char::from_u32)
                .filter(|c| !c.is_control() || matches!(c, '\n' | '\r' | '\t'))
                .ok_or_else(bad)
        }
    }
}
fn declaration(d: &quick_xml::events::BytesDecl<'_>) -> Result<(), BookError> {
    if d.version().map_err(|_| bad())?.as_ref() != b"1.0" {
        return Err(bad());
    }
    if let Some(e) = d.encoding() {
        let e = e.map_err(|_| bad())?;
        if !e.eq_ignore_ascii_case(b"UTF-8") && !e.eq_ignore_ascii_case(b"UTF8") {
            return Err(bad());
        }
    }
    Ok(())
}
fn content_type(xml: &str) -> Result<(), BookError> {
    let mut r = NsReader::from_str(xml);
    let mut found = false;
    let mut depth = 0usize;
    let mut root = false;
    loop {
        let event = r.read_event().map_err(|_| bad())?;
        let ns = match &event {
            Event::Start(e) | Event::Empty(e) => {
                attributes(&r, e)?;
                r.resolver().resolve_element(e.name()).0
            }
            Event::End(e) => r.resolver().resolve_element(e.name()).0,
            _ => ResolveResult::Unbound,
        };
        if matches!(ns, ResolveResult::Unknown(_)) {
            return Err(bad());
        }
        match event {
            Event::Start(ref e) | Event::Empty(ref e) => {
                if depth == 0 {
                    if root
                        || e.local_name().as_ref() != b"Types"
                        || !matches!(ns,ResolveResult::Bound(n) if n.as_ref()==b"http://schemas.openxmlformats.org/package/2006/content-types")
                    {
                        return Err(bad());
                    }
                    root = true;
                }
                if e.local_name().as_ref() == b"Override" {
                    if depth != 1
                        || !matches!(ns,ResolveResult::Bound(n) if n.as_ref()==b"http://schemas.openxmlformats.org/package/2006/content-types")
                    {
                        return Err(bad());
                    }
                    let mut path = None;
                    let mut mime = None;
                    for a in e.attributes() {
                        let a = a.map_err(|_| bad())?;
                        let v = a
                            .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                            .map_err(|_| bad())?
                            .into_owned();
                        match a.key.as_ref() {
                            b"PartName" => path = Some(v),
                            b"ContentType" => mime = Some(v),
                            _ => {}
                        }
                    }
                    if path.as_deref() == Some("/word/document.xml") {
                        if found
                            || mime.as_deref()
                                != Some(
                                    "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
                                )
                        {
                            return Err(bad());
                        }
                        found = true;
                    }
                }
                if matches!(event, Event::Start(_)) {
                    depth += 1;
                    if depth > 64 {
                        return Err(BookError::TooLarge);
                    }
                }
            }
            Event::End(_) => {
                depth = depth.checked_sub(1).ok_or_else(bad)?;
            }
            Event::Decl(d) => declaration(&d)?,
            Event::Text(e) if depth == 0 && !e.decode().map_err(|_| bad())?.trim().is_empty() => {
                return Err(bad());
            }
            Event::DocType(_) | Event::GeneralRef(_) => return Err(bad()),
            Event::CData(_) if depth == 0 => return Err(bad()),
            Event::Eof => break,
            _ => {}
        }
    }
    if found && depth == 0 {
        Ok(())
    } else {
        Err(bad())
    }
}
fn body(xml: &str, cancel: &Cancel) -> Result<Chapter, BookError> {
    let mut r = NsReader::from_str(xml);
    r.config_mut().expand_empty_elements = true;
    let mut depth = 0usize;
    let mut root = false;
    let mut body = None;
    let mut body_seen = false;
    let mut skipped = None;
    let mut paragraph = None::<(usize, Paragraph)>;
    let mut text_depth = None;
    let mut out = Chapter::default();
    let mut total = 0usize;
    loop {
        crate::analysis::check_cancel(cancel)?;
        let event = r.read_event().map_err(|_| bad())?;
        let ns = match &event {
            Event::Start(e) => {
                attributes(&r, e)?;
                r.resolver().resolve_element(e.name()).0
            }
            Event::End(e) => r.resolver().resolve_element(e.name()).0,
            _ => ResolveResult::Unbound,
        };
        if matches!(ns, ResolveResult::Unknown(_)) {
            return Err(bad());
        }
        match event {
            Event::Start(e) => {
                depth += 1;
                if depth > 64 {
                    return Err(BookError::TooLarge);
                }
                let local = e.local_name();
                let name = local.as_ref();
                let w = word(&ns);
                if depth == 1 {
                    if root || !w || name != b"document" {
                        return Err(bad());
                    }
                    root = true;
                }
                if w && name == b"body" {
                    if depth != 2 || body_seen {
                        return Err(bad());
                    }
                    body_seen = true;
                    body = Some(depth);
                }
                if body.is_none() || skipped.is_some() {
                    continue;
                }
                if w && matches!(
                    name,
                    b"del" | b"drawing" | b"pict" | b"object" | b"txbxContent" | b"instrText"
                ) {
                    skipped = Some(depth);
                    continue;
                }
                if !w {
                    continue;
                }
                match name {
                    b"p" => {
                        if paragraph.is_some() {
                            return Err(bad());
                        }
                        paragraph = Some((depth, Paragraph::default()));
                    }
                    b"t" if paragraph.is_some() => text_depth = Some(depth),
                    b"tab" if paragraph.is_some() => {
                        paragraph.as_mut().ok_or_else(bad)?.1.text.push('\t')
                    }
                    b"br" | b"cr" if paragraph.is_some() => {
                        paragraph.as_mut().ok_or_else(bad)?.1.text.push('\n')
                    }
                    b"pStyle" if paragraph.is_some() => {
                        for a in e.attributes() {
                            let a = a.map_err(|_| bad())?;
                            if a.key.local_name().as_ref() == b"val"
                                && word(&r.resolver().resolve_attribute(a.key).0)
                            {
                                let v = a
                                    .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                                    .map_err(|_| bad())?;
                                let lower = v.to_ascii_lowercase();
                                if lower.strip_prefix("heading").is_some_and(|s| {
                                    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
                                }) {
                                    paragraph.as_mut().ok_or_else(bad)?.1.heading = true;
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            Event::Text(e) if skipped.is_none() && text_depth.is_some() => {
                let text = e
                    .xml_content(quick_xml::XmlVersion::Implicit1_0)
                    .map_err(|_| bad())?;
                if let Some((_, p)) = &mut paragraph {
                    p.text.push_str(&text);
                    if p.text.len() > crate::text::MAX_PARAGRAPH_BYTES {
                        return Err(BookError::TooLarge);
                    }
                }
            }
            Event::CData(e) if skipped.is_none() && text_depth.is_some() => {
                if let Some((_, p)) = &mut paragraph {
                    p.text.push_str(
                        &e.xml_content(quick_xml::XmlVersion::Implicit1_0)
                            .map_err(|_| bad())?,
                    );
                    if p.text.len() > crate::text::MAX_PARAGRAPH_BYTES {
                        return Err(BookError::TooLarge);
                    }
                }
            }
            Event::GeneralRef(e) => {
                let c = reference(e.as_ref())?;
                if skipped.is_none()
                    && text_depth.is_some()
                    && let Some((_, p)) = &mut paragraph
                {
                    p.text.push(c);
                }
            }
            Event::End(_) => {
                if skipped == Some(depth) {
                    skipped = None;
                } else if skipped.is_none() {
                    if text_depth == Some(depth) {
                        text_depth = None;
                    }
                    if paragraph.as_ref().is_some_and(|(d, _)| *d == depth) {
                        let p = paragraph.take().ok_or_else(bad)?.1;
                        total += p.text.len();
                        if out.paragraphs.len() >= crate::text::MAX_PARAGRAPHS
                            || total > crate::text::MAX_BYTES
                        {
                            return Err(BookError::TooLarge);
                        }
                        out.paragraphs.push(p);
                    }
                }
                if body == Some(depth) {
                    body = None;
                }
                depth = depth.checked_sub(1).ok_or_else(bad)?;
            }
            Event::Decl(d) => declaration(&d)?,
            Event::Text(e) if depth == 0 && !e.decode().map_err(|_| bad())?.trim().is_empty() => {
                return Err(bad());
            }
            Event::DocType(_) => return Err(bad()),
            Event::CData(_) if depth == 0 => return Err(bad()),
            Event::Eof => break,
            _ => {}
        }
    }
    if !root || !body_seen || depth != 0 || paragraph.is_some() {
        return Err(bad());
    }
    Ok(out)
}
pub(super) fn read(bytes: &[u8], cancel: &Cancel) -> Result<Vec<Chapter>, BookError> {
    let mut a = Archive::new(bytes, cancel)?;
    content_type(&a.text("[Content_Types].xml", cancel)?)?;
    Ok(vec![body(&a.text("word/document.xml", cancel)?, cancel)?])
}
