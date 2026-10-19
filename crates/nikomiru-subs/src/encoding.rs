// SPDX-License-Identifier: GPL-3.0-only

//! The encoding of a subtitle file (SUB-005): a byte-order mark, then UTF-8
//! validation, then the legacy encodings, choosing the first that decodes
//! without errors with the highest share of plausible characters. The legacy
//! conversions come from a [`Decoder`] (glibc `iconv` in the application), so
//! this crate stays free of FFI (ARC-011).

/// Legacy encodings tried in this order (SUB-005), with `iconv` names.
pub const LEGACY: [&str; 5] = ["CP932", "EUC-JP", "GB18030", "BIG5", "CP1252"];

/// Converts bytes from a named encoding.
pub trait Decoder {
    /// `bytes` decoded from `encoding`, or `None` if they are not valid in it.
    fn decode(&self, bytes: &[u8], encoding: &str) -> Option<String>;
}

/// A decoded file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decoded {
    /// The text, without a byte-order mark.
    pub text: String,
    /// The encoding that was used (shown to the user, SUB-005).
    pub encoding: String,
}

fn utf16(bytes: &[u8], little_endian: bool) -> Option<String> {
    let units: Vec<u16> = bytes
        .chunks(2)
        .map(|c| match (c, little_endian) {
            ([a, b], true) => Some(u16::from_le_bytes([*a, *b])),
            ([a, b], false) => Some(u16::from_be_bytes([*a, *b])),
            _ => None,
        })
        .collect::<Option<_>>()?;
    String::from_utf16(&units).ok()
}

/// Decodes with the encodings Rust knows (UTF-8 and UTF-16); others go to
/// `decoder`.
fn decode_with(bytes: &[u8], encoding: &str, decoder: &dyn Decoder) -> Option<String> {
    match encoding.to_ascii_uppercase().replace('_', "-").as_str() {
        "UTF-8" | "UTF8" => {
            let b = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
            String::from_utf8(b.to_vec()).ok()
        }
        "UTF-16LE" => utf16(bytes.strip_prefix(b"\xFF\xFE").unwrap_or(bytes), true),
        "UTF-16BE" => utf16(bytes.strip_prefix(b"\xFE\xFF").unwrap_or(bytes), false),
        _ => decoder.decode(bytes, encoding),
    }
}

/// How plausible a text is as subtitles, per character: kana and common
/// punctuation count most, CJK ideographs and printable Latin count, control
/// characters, replacement characters, private-use and half-width katakana
/// (the usual result of a wrong guess) count against.
pub fn plausibility(text: &str) -> f64 {
    let mut score = 0.0;
    let mut chars = 0usize;
    for c in text.chars() {
        chars += 1;
        score += match c {
            '\u{3040}'..='\u{30ff}' => 2.0,
            '\u{3000}'..='\u{303f}' | '\u{ff01}'..='\u{ff60}' => 1.5,
            '\u{4e00}'..='\u{9fff}' | '\u{ac00}'..='\u{d7af}' => 1.0,
            ' '..='~' | '\t' | '\n' | '\r' => 1.0,
            '\u{a0}'..='\u{ff}' if c.is_alphabetic() => 1.0,
            '\u{ff61}'..='\u{ff9f}' => -1.0,
            '\u{fffd}' | '\u{e000}'..='\u{f8ff}' => -5.0,
            c if c.is_control() => -5.0,
            c if c.is_alphabetic() => 0.5,
            _ => 0.0,
        };
    }
    if chars == 0 {
        0.0
    } else {
        score / chars as f64
    }
}

/// Decodes a subtitle file. `forced` (chosen by the user) takes precedence;
/// otherwise a BOM, valid UTF-8, then the most plausible of [`LEGACY`]
/// (ties go to the earlier one).
pub fn decode(bytes: &[u8], forced: Option<&str>, decoder: &dyn Decoder) -> Option<Decoded> {
    let done = |text: String, encoding: &str| Decoded {
        text: text
            .strip_prefix('\u{feff}')
            .map(str::to_string)
            .unwrap_or(text),
        encoding: encoding.to_string(),
    };
    if let Some(encoding) = forced {
        return decode_with(bytes, encoding, decoder).map(|t| done(t, encoding));
    }
    for (bom, encoding) in [
        (&b"\xEF\xBB\xBF"[..], "UTF-8"),
        (&b"\xFF\xFE"[..], "UTF-16LE"),
        (&b"\xFE\xFF"[..], "UTF-16BE"),
    ] {
        if bytes.starts_with(bom) {
            return decode_with(bytes, encoding, decoder).map(|t| done(t, encoding));
        }
    }
    if let Ok(text) = std::str::from_utf8(bytes) {
        return Some(done(text.to_string(), "UTF-8"));
    }
    let mut best: Option<(f64, String, &str)> = None;
    for encoding in LEGACY {
        if let Some(text) = decoder.decode(bytes, encoding) {
            let score = plausibility(&text);
            if best.as_ref().is_none_or(|(s, _, _)| score > *s) {
                best = Some((score, text, encoding));
            }
        }
    }
    best.map(|(_, text, encoding)| done(text, encoding))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Knows only Latin-1 (as "CP1252") and a fake "CP932" that maps 0x82 0xA0
    /// to あ, so the choice logic can be tested without iconv.
    struct Fake;

    impl Decoder for Fake {
        fn decode(&self, bytes: &[u8], encoding: &str) -> Option<String> {
            match encoding {
                "CP1252" => Some(bytes.iter().map(|&b| char::from(b)).collect()),
                "CP932" => {
                    let mut out = String::new();
                    let mut it = bytes.iter();
                    while let Some(&b) = it.next() {
                        match b {
                            0x82 => (*it.next()? == 0xA0).then(|| out.push('あ'))?,
                            b if b < 0x80 => out.push(char::from(b)),
                            _ => return None,
                        }
                    }
                    Some(out)
                }
                _ => None,
            }
        }
    }

    #[test]
    fn boms_utf8_and_forced_encodings() {
        let utf16 = [0xFF, 0xFE, 0x42, 0x30, b'!', 0];
        assert_eq!(decode(&utf16, None, &Fake).unwrap().text, "あ!");
        let d = decode("\u{feff}テスト".as_bytes(), None, &Fake).unwrap();
        assert_eq!((d.text.as_str(), d.encoding.as_str()), ("テスト", "UTF-8"));
        // a forced encoding wins over detection
        let d = decode(b"caf\xE9", Some("CP1252"), &Fake).unwrap();
        assert_eq!(d.text, "café");
        assert!(decode(b"\xFF", Some("UTF-8"), &Fake).is_none());
    }

    #[test]
    fn the_most_plausible_legacy_encoding_wins() {
        let sjis = [0x82, 0xA0, 0x82, 0xA0, b'!'];
        let d = decode(&sjis, None, &Fake).unwrap();
        assert_eq!((d.text.as_str(), d.encoding.as_str()), ("ああ!", "CP932"));
        let latin = b"Bonjour \xE0 tous";
        assert_eq!(decode(latin, None, &Fake).unwrap().encoding, "CP1252");
        assert!(plausibility("あいう") > plausibility("ｱｲｳ"));
    }
}
