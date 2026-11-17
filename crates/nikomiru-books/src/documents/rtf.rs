// SPDX-License-Identifier: GPL-3.0-only

//! Bounded original RTF token reader: scoped Unicode/legacy runs and inert destinations.
use super::*;
use crate::Paragraph;
#[derive(Clone)]
struct State {
    skip: bool,
    font_table: bool,
    font: Option<i32>,
    page: u16,
    uc: usize,
    upright: bool,
    children: usize,
    unicode_alternative: bool,
}
impl Default for State {
    fn default() -> Self {
        Self {
            skip: false,
            font_table: false,
            font: None,
            page: 1252,
            uc: 1,
            upright: false,
            children: 0,
            unicode_alternative: false,
        }
    }
}
struct Reader<'a> {
    support: &'a dyn Support,
    states: Vec<State>,
    fonts: HashMap<i32, u16>,
    default_font: Option<i32>,
    raw: Vec<u8>,
    paragraph: String,
    chapter: Chapter,
    total: usize,
    high: Option<u16>,
    fallback: usize,
}
impl Reader<'_> {
    fn state(&self) -> Result<&State, BookError> {
        self.states.last().ok_or_else(bad)
    }
    fn append(&mut self, text: &str) -> Result<(), BookError> {
        if self.high.is_some() {
            return Err(bad());
        }
        if text
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\t' | '\r'))
        {
            return Err(bad());
        }
        self.total = self
            .total
            .checked_add(text.len())
            .ok_or(BookError::TooLarge)?;
        if self.total > crate::text::MAX_BYTES
            || self.paragraph.len() + text.len() > crate::text::MAX_PARAGRAPH_BYTES
        {
            return Err(BookError::TooLarge);
        }
        self.paragraph.push_str(text);
        Ok(())
    }
    fn flush(&mut self) -> Result<(), BookError> {
        if self.raw.is_empty() {
            return Ok(());
        }
        let state = self.state()?;
        let page = state
            .font
            .and_then(|f| self.fonts.get(&f))
            .copied()
            .unwrap_or(state.page);
        if page == 0 {
            return Err(bad());
        }
        let text = self
            .support
            .decode(&self.raw, &format!("CP{page}"))
            .ok_or_else(bad)?;
        self.raw.clear();
        self.append(&text)
    }
    fn byte(&mut self, b: u8) -> Result<(), BookError> {
        if self.fallback > 0 {
            self.fallback -= 1;
            return Ok(());
        }
        if !self.state()?.skip {
            if self.raw.len() >= crate::text::MAX_PARAGRAPH_BYTES {
                return Err(BookError::TooLarge);
            }
            self.raw.push(b);
        }
        Ok(())
    }
    fn character(&mut self, c: char) -> Result<(), BookError> {
        if self.fallback > 0 {
            self.fallback -= 1;
            return Ok(());
        }
        self.flush()?;
        if !self.state()?.skip {
            self.append(c.encode_utf8(&mut [0; 4]))?;
        }
        Ok(())
    }
    fn unit(&mut self, u: u16) -> Result<(), BookError> {
        self.flush()?;
        if self.state()?.skip {
            return Ok(());
        }
        match u {
            0xd800..=0xdbff => {
                if self.high.replace(u).is_some() {
                    return Err(bad());
                }
            }
            0xdc00..=0xdfff => {
                let h = self.high.take().ok_or_else(bad)?;
                let c = char::from_u32(
                    0x10000 + ((u32::from(h) - 0xd800) << 10) + (u32::from(u) - 0xdc00),
                )
                .ok_or_else(bad)?;
                self.append(c.encode_utf8(&mut [0; 4]))?;
            }
            _ => self.append(
                char::from_u32(u32::from(u))
                    .ok_or_else(bad)?
                    .encode_utf8(&mut [0; 4]),
            )?,
        }
        Ok(())
    }
    fn end_paragraph(&mut self) -> Result<(), BookError> {
        self.flush()?;
        if self.state()?.skip {
            return Ok(());
        }
        if self.high.is_some() {
            return Err(bad());
        }
        if self.chapter.paragraphs.len() >= crate::text::MAX_PARAGRAPHS {
            return Err(BookError::TooLarge);
        }
        self.chapter.paragraphs.push(Paragraph {
            text: std::mem::take(&mut self.paragraph),
            ..Default::default()
        });
        Ok(())
    }
    fn control(&mut self, word: &str, n: Option<i32>) -> Result<(), BookError> {
        self.flush()?;
        let arg = || n.ok_or_else(bad);
        match word {
            "rtf" => {
                if n != Some(1) {
                    return Err(bad());
                }
            }
            "fonttbl" => {
                let s = self.states.last_mut().ok_or_else(bad)?;
                s.skip = true;
                s.font_table = true;
            }
            "deff" => {
                let id = arg()?;
                if !(0..=65535).contains(&id) {
                    return Err(bad());
                }
                self.default_font = Some(id);
                if self.state()?.font.is_none() {
                    self.states.last_mut().ok_or_else(bad)?.font = Some(id);
                }
            }
            "plain" => self.states.last_mut().ok_or_else(bad)?.font = self.default_font,
            "f" => {
                let id = arg()?;
                if !(0..=65535).contains(&id) {
                    return Err(bad());
                }
                self.states.last_mut().ok_or_else(bad)?.font = Some(id);
            }
            "fcharset" if self.state()?.font_table => {
                let page = match arg()? {
                    0 | 1 => self.state()?.page,
                    128 => 932,
                    129 => 949,
                    130 => 1361,
                    134 => 936,
                    136 => 950,
                    161 => 1253,
                    162 => 1254,
                    163 => 1258,
                    177 => 1255,
                    178 => 1256,
                    186 => 1257,
                    204 => 1251,
                    222 => 874,
                    238 => 1250,
                    _ => 0,
                };
                let font = self.state()?.font.ok_or_else(bad)?;
                if self.fonts.len() >= 2048 && !self.fonts.contains_key(&font) {
                    return Err(BookError::TooLarge);
                }
                self.fonts.insert(font, page);
            }
            "cpg" => {
                let page = u16::try_from(arg()?)
                    .ok()
                    .filter(|p| *p > 0)
                    .ok_or_else(bad)?;
                if self.state()?.font_table {
                    let font = self.state()?.font.ok_or_else(bad)?;
                    if self.fonts.len() >= 2048 && !self.fonts.contains_key(&font) {
                        return Err(BookError::TooLarge);
                    }
                    self.fonts.insert(font, page);
                } else {
                    self.states.last_mut().ok_or_else(bad)?.page = page;
                }
            }
            "ansicpg" => {
                let p = u16::try_from(arg()?)
                    .ok()
                    .filter(|p| *p > 0)
                    .ok_or_else(bad)?;
                self.states.last_mut().ok_or_else(bad)?.page = p;
            }
            "ansi" => self.states.last_mut().ok_or_else(bad)?.page = 1252,
            "mac" | "pc" | "pca" => self.states.last_mut().ok_or_else(bad)?.page = 0,
            "uc" => {
                let uc = usize::try_from(arg()?)
                    .ok()
                    .filter(|n| *n <= 32)
                    .ok_or_else(bad)?;
                self.states.last_mut().ok_or_else(bad)?.uc = uc;
            }
            "u" => {
                let value = arg()?;
                if !(-32768..=65535).contains(&value) {
                    return Err(bad());
                }
                self.unit(value as u16)?;
                self.fallback = self.state()?.uc;
            }
            "par" | "row" => {
                if self.fallback > 0 {
                    self.fallback -= 1;
                } else {
                    self.end_paragraph()?;
                }
            }
            "line" => self.character('\n')?,
            "tab" | "cell" => self.character('\t')?,
            "emdash" => self.character('—')?,
            "endash" => self.character('–')?,
            "bullet" => self.character('•')?,
            "lquote" => self.character('‘')?,
            "rquote" => self.character('’')?,
            "ldblquote" => self.character('“')?,
            "rdblquote" => self.character('”')?,
            "upr" => self.states.last_mut().ok_or_else(bad)?.upright = true,
            "ud" if self.state()?.unicode_alternative => {
                let s = self.states.last_mut().ok_or_else(bad)?;
                s.skip = false;
                s.font_table = false;
                s.unicode_alternative = false;
            }
            "colortbl" | "stylesheet" | "info" | "pict" | "object" | "objdata" | "fldinst"
            | "datafield" | "generator" | "filetbl" | "revtbl" | "rsidtbl" | "listtable"
            | "listoverridetable" | "listtext" | "pntext" | "xmlnstbl" | "datastore"
            | "themedata" | "colorschememapping" | "header" | "headerl" | "headerr" | "headerf"
            | "footer" | "footerl" | "footerr" | "footerf" | "footnote" | "annotation" | "shp"
            | "nonshppict" => {
                let s = self.states.last_mut().ok_or_else(bad)?;
                s.skip = true;
                s.font_table = false;
            }
            _ => {}
        }
        Ok(())
    }
}
fn number(bytes: &[u8], at: &mut usize) -> Result<Option<i32>, BookError> {
    let start = *at;
    let negative = bytes.get(*at) == Some(&b'-');
    if negative && bytes.get(*at + 1).is_some_and(u8::is_ascii_digit) {
        *at += 1;
    }
    let digits = *at;
    while bytes.get(*at).is_some_and(u8::is_ascii_digit) {
        *at += 1;
        if *at - start > 11 {
            return Err(bad());
        }
    }
    if *at == digits {
        return Ok(None);
    }
    std::str::from_utf8(&bytes[start..*at])
        .ok()
        .and_then(|s| s.parse().ok())
        .map(Some)
        .ok_or_else(bad)
}
pub(super) fn read(
    bytes: &[u8],
    support: &dyn Support,
    cancel: &Cancel,
) -> Result<Vec<Chapter>, BookError> {
    if !bytes.starts_with(b"{\\rtf1") || bytes.get(6).is_some_and(u8::is_ascii_digit) {
        return Err(bad());
    }
    let mut r = Reader {
        support,
        states: Vec::new(),
        fonts: HashMap::new(),
        default_font: None,
        raw: Vec::new(),
        paragraph: String::new(),
        chapter: Chapter::default(),
        total: 0,
        high: None,
        fallback: 0,
    };
    let mut i = 0usize;
    let mut closed = false;
    while i < bytes.len() {
        crate::analysis::check_cancel(cancel)?;
        let b = bytes[i];
        i += 1;
        if closed {
            if !b.is_ascii_whitespace() {
                return Err(bad());
            }
            continue;
        }
        match b {
            b'{' => {
                if !r.states.is_empty() {
                    r.flush()?;
                }
                if r.states.len() >= 64 {
                    return Err(BookError::TooLarge);
                }
                let mut state = r.states.last().cloned().unwrap_or_default();
                if let Some(parent) = r.states.last_mut()
                    && parent.upright
                {
                    parent.children += 1;
                    state.skip = true;
                    state.font_table = false;
                    state.unicode_alternative = parent.children == 2 && !parent.skip;
                }
                state.upright = false;
                state.children = 0;
                r.states.push(state);
                r.fallback = 0;
            }
            b'}' => {
                r.flush()?;
                r.states.pop().ok_or_else(bad)?;
                r.fallback = 0;
                if r.states.is_empty() {
                    closed = true;
                }
            }
            b'\r' | b'\n' => {}
            b'\\' => {
                let symbol = *bytes.get(i).ok_or_else(bad)?;
                i += 1;
                match symbol {
                    b'\\' | b'{' | b'}' => r.byte(symbol)?,
                    b'\'' => {
                        let hex = bytes.get(i..i + 2).ok_or_else(bad)?;
                        let n = std::str::from_utf8(hex)
                            .ok()
                            .and_then(|h| u8::from_str_radix(h, 16).ok())
                            .ok_or_else(bad)?;
                        i += 2;
                        r.byte(n)?;
                    }
                    b'*' => {
                        r.flush()?;
                        let s = r.states.last_mut().ok_or_else(bad)?;
                        s.skip = true;
                        s.font_table = false;
                    }
                    b'~' => r.character('\u{a0}')?,
                    b'_' => r.character('‑')?,
                    b'-' => {}
                    b'\r' | b'\n' => {}
                    c if c.is_ascii_alphabetic() => {
                        let start = i - 1;
                        while bytes.get(i).is_some_and(u8::is_ascii_alphabetic) {
                            i += 1;
                            if i - start > 32 {
                                return Err(bad());
                            }
                        }
                        let word = std::str::from_utf8(&bytes[start..i]).map_err(|_| bad())?;
                        let n = number(bytes, &mut i)?;
                        if bytes.get(i) == Some(&b' ') {
                            i += 1;
                        }
                        if word == "bin" {
                            r.flush()?;
                            let len = n.and_then(|n| usize::try_from(n).ok()).ok_or_else(bad)?;
                            i = i
                                .checked_add(len)
                                .filter(|end| *end <= bytes.len())
                                .ok_or_else(bad)?;
                            r.fallback = 0;
                        } else {
                            r.control(word, n)?;
                        }
                    }
                    _ => {}
                }
            }
            byte => r.byte(byte)?,
        }
    }
    if !closed || !r.states.is_empty() || r.high.is_some() {
        return Err(bad());
    }
    if !r.paragraph.is_empty() {
        if r.chapter.paragraphs.len() >= crate::text::MAX_PARAGRAPHS {
            return Err(BookError::TooLarge);
        }
        r.chapter.paragraphs.push(Paragraph {
            text: r.paragraph,
            ..Default::default()
        });
    }
    Ok(vec![r.chapter])
}
