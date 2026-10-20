// SPDX-License-Identifier: GPL-3.0-only

//! The link back to the scene (ANK-050…ANK-052):
//! `nikomiru://source?v=1&kind=video&uid=…&ep=…&startMs=…&endMs=…&sourceId=…`,
//! and its strict parser.
//!
//! The rules for the values are Fushi's
//! (`packages/fushi_anki/lib/src/card_source_link.dart`, commit `ac8034e`):
//! integers without sign or leading zeros up to 2³¹−1, a lowercase UUID, a uid
//! that cannot name a path. Nikomiru has no fingerprint and calls the episode
//! `ep`.

use crate::html::escape;

/// The longest link accepted (ANK-051).
pub const MAX_LENGTH: usize = 16 * 1024;

/// The longest uid accepted.
const MAX_UID: usize = 1024;

/// A link to a scene of a video.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceLink {
    /// The video's uid in the library (DAT-003).
    pub uid: String,
    /// The episode (0 until series exist, M5).
    pub episode: u32,
    /// Start of the clip, in milliseconds of the video.
    pub start_ms: u32,
    /// End of the clip.
    pub end_ms: u32,
    /// The card's own id, a lowercase UUID v4.
    pub source_id: String,
}

/// Why a link was refused (ANK-051); the reason goes to the log only.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("not a link to a scene: {0}")]
pub struct LinkError(pub String);

impl nikomiru_core::Localized for LinkError {
    fn message_id(&self) -> &'static str {
        "error-link-invalid"
    }

    fn message_args(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}

/// A new card id.
pub fn new_source_id() -> String {
    uuid::Uuid::new_v4().hyphenated().to_string()
}

/// Percent-encodes everything but the unreserved characters (RFC 3986).
fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Decodes `%XX` and `+`; `None` for a broken escape or bytes that are not UTF-8.
fn decode(text: &str) -> Option<String> {
    let mut bytes = Vec::with_capacity(text.len());
    let mut it = text.bytes();
    while let Some(b) = it.next() {
        match b {
            b'%' => {
                let hex = [it.next()?, it.next()?];
                let hex = std::str::from_utf8(&hex).ok()?;
                bytes.push(u8::from_str_radix(hex, 16).ok()?);
            }
            b'+' => bytes.push(b' '),
            b => bytes.push(b),
        }
    }
    String::from_utf8(bytes).ok()
}

fn integer(text: &str) -> Option<u32> {
    let ok = text == "0"
        || (text.len() <= 10
            && text.starts_with(|c: char| ('1'..='9').contains(&c))
            && text.bytes().all(|b| b.is_ascii_digit()));
    ok.then(|| text.parse::<u32>().ok())
        .flatten()
        .filter(|&n| n <= i32::MAX as u32)
}

fn is_uuid(text: &str) -> bool {
    let groups: Vec<&str> = text.split('-').collect();
    groups.iter().map(|g| g.len()).eq([8, 4, 4, 4, 12])
        && groups.iter().all(|g| {
            g.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
}

/// Whether a uid could only name a library entry, never a file.
fn is_safe_uid(uid: &str) -> bool {
    let bytes = uid.as_bytes();
    !uid.is_empty()
        && uid.chars().count() <= MAX_UID
        && !uid.starts_with('/')
        && !(bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':')
        && !uid.to_ascii_lowercase().starts_with("file:")
        && !uid.contains('\\')
        && !uid.chars().any(char::is_control)
        && !uid.split('/').any(|s| s == "." || s == "..")
}

impl SourceLink {
    /// The link (ANK-050).
    pub fn to_url(&self) -> String {
        format!(
            "nikomiru://source?v=1&kind=video&uid={}&ep={}&startMs={}&endMs={}&sourceId={}",
            encode(&self.uid),
            self.episode,
            self.start_ms,
            self.end_ms,
            self.source_id
        )
    }

    /// `{source-link}`: the link with `title` as its text, both escaped.
    pub fn to_html(&self, title: &str) -> String {
        let title = if title.trim().is_empty() {
            "Nikomiru"
        } else {
            title
        };
        format!(
            r#"<a href="{}">{}</a>"#,
            escape(&self.to_url()),
            escape(title)
        )
    }

    /// Reads a link, strictly (ANK-051): unknown or repeated parameters, a
    /// path, a port, a user, a fragment, a uid that names a file, a value out
    /// of range: refused.
    pub fn parse(url: &str) -> Result<Self, LinkError> {
        let bad = |why: &str| LinkError(why.to_string());
        if url.len() > MAX_LENGTH {
            return Err(bad("too long"));
        }
        let (scheme, rest) = url.split_once("://").ok_or_else(|| bad("no scheme"))?;
        if !scheme.eq_ignore_ascii_case("nikomiru") {
            return Err(bad("not nikomiru://"));
        }
        if rest.contains('#') {
            return Err(bad("a fragment"));
        }
        let (authority, query) = rest.split_once('?').ok_or_else(|| bad("no query"))?;
        let host = authority.strip_suffix('/').unwrap_or(authority);
        if !host.eq_ignore_ascii_case("source") {
            return Err(bad("not source, or with a path, port or user"));
        }
        let mut pairs: Vec<(&str, &str)> = Vec::new();
        for part in query.split('&') {
            let (k, v) = part
                .split_once('=')
                .ok_or_else(|| bad("a parameter without value"))?;
            if !["v", "kind", "uid", "ep", "startMs", "endMs", "sourceId"].contains(&k) {
                return Err(bad(&format!("unknown parameter {k}")));
            }
            if pairs.iter().any(|(seen, _)| *seen == k) {
                return Err(bad(&format!("{k} twice")));
            }
            pairs.push((k, v));
        }
        let get = |key: &str| {
            pairs
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| *v)
                .ok_or_else(|| bad(&format!("no {key}")))
        };
        let number =
            |key: &str| integer(get(key)?).ok_or_else(|| bad(&format!("{key} is not a number")));
        if get("v")? != "1" {
            return Err(bad("version"));
        }
        if get("kind")? != "video" {
            return Err(bad("kind"));
        }
        let uid = decode(get("uid")?).ok_or_else(|| bad("uid encoding"))?;
        if !is_safe_uid(&uid) {
            return Err(bad("uid"));
        }
        let episode = number("ep")?;
        let start_ms = number("startMs")?;
        let end_ms = number("endMs")?;
        if end_ms < start_ms {
            return Err(bad("end before start"));
        }
        let source_id = get("sourceId")?.to_string();
        if !is_uuid(&source_id) {
            return Err(bad("sourceId"));
        }
        Ok(SourceLink {
            uid,
            episode,
            start_ms,
            end_ms,
            source_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link() -> SourceLink {
        SourceLink {
            uid: "video/夏目友人帳 第1話 (2)".into(),
            episode: 0,
            start_ms: 754_000,
            end_ms: 758_000,
            source_id: "12345678-1234-4234-8234-123456789abc".into(),
        }
    }

    // T-ANK-050
    #[test]
    fn a_link_round_trips() {
        let url = link().to_url();
        assert_eq!(
            url,
            "nikomiru://source?v=1&kind=video&uid=video%2F%E5%A4%8F%E7%9B%AE%E5%8F%8B%E4%BA%BA%E5%B8%B3%20%E7%AC%AC1%E8%A9%B1%20%282%29&ep=0&startMs=754000&endMs=758000&sourceId=12345678-1234-4234-8234-123456789abc"
        );
        assert_eq!(SourceLink::parse(&url), Ok(link()));
        assert_eq!(
            link().to_html("A <b>title</b>"),
            format!(
                r#"<a href="{}">A &lt;b&gt;title&lt;/b&gt;</a>"#,
                url.replace('&', "&amp;")
            )
        );
        // the scheme and host in capitals, `/` before the query, `+` for spaces
        let loose = url
            .replace("nikomiru://source?", "NIKOMIRU://Source/?")
            .replace("%20", "+");
        assert_eq!(SourceLink::parse(&loose), Ok(link()));
        assert!(is_uuid(&new_source_id()));
    }

    // T-ANK-051, T-SEC-007: Fushi `card_source_link_test.dart`
    #[test]
    fn hostile_links_are_refused() {
        let valid = link().to_url();
        let cases = [
            valid.replace("v=1", "v=2"),
            valid.replace("kind=video", "kind=book"),
            valid.replace("startMs=754000", "startMs=-1"),
            valid.replace("startMs=754000", "startMs=1.5"),
            valid.replace("startMs=754000", "startMs=2147483648"),
            valid.replace("startMs=754000", "startMs=01"),
            valid.replace("endMs=758000", "endMs=1"),
            valid.replace("sourceId=12345678", "sourceId=ABCDEF78"),
            format!("{valid}&v=1"),
            format!("{valid}&command=delete"),
            format!("{valid}#fragment"),
            valid.replace("nikomiru://source", "nikomiru://source/path"),
            valid.replace("nikomiru://source", "nikomiru://user@source"),
            valid.replace("nikomiru://source", "nikomiru://source:80"),
            valid.replace("ep=0&", ""),
            "file:///tmp/video.mkv".to_string(),
            "fushi://source?v=1".to_string(),
            format!("{valid}&{}", "x".repeat(MAX_LENGTH)),
        ];
        for raw in &cases {
            assert!(SourceLink::parse(raw).is_err(), "{raw}");
        }
        for uid in [
            "/home/u/video.mkv",
            "C%3A%5Csecret.mkv",
            "file%3A%2F%2F%2Ftmp%2Fx",
            "..%2Fx",
            "video%2F..%2Fx",
            "video%2F.%2Fx",
            "a%00b",
            "a%5Cb",
            "",
            "%E0%A4",
            "%ZZ",
        ] {
            let raw = valid.replace(
                &format!("uid={}", encode(&link().uid)),
                &format!("uid={uid}"),
            );
            assert!(SourceLink::parse(&raw).is_err(), "{uid}");
        }
        let long = valid.replace(
            &format!("uid={}", encode(&link().uid)),
            &format!("uid=video%2F{}", "a".repeat(MAX_UID)),
        );
        assert!(SourceLink::parse(&long).is_err());
    }

    // T-ERR-001
    #[test]
    fn the_error_has_its_message() {
        let e = LinkError("x".into());
        let dir = std::path::Path::new(nikomiru_core::error::I18N_DIR);
        let problems = nikomiru_core::error::message_problems(dir, &[&e]);
        assert!(problems.is_empty(), "{problems:#?}");
    }

    proptest::proptest! {
        // T-FUZZ-007: any text, and any change to a valid link, is read
        // without a panic; what is accepted reads back the same
        #[test]
        fn the_parser_never_panics(text in ".{0,300}", at in 0usize..400, insert in ".{0,8}") {
            let _ = SourceLink::parse(&text);
            let mut url = link().to_url();
            let mut cut = at.min(url.len());
            while !url.is_char_boundary(cut) {
                cut -= 1;
            }
            url.insert_str(cut, &insert);
            if let Ok(parsed) = SourceLink::parse(&url) {
                proptest::prop_assert_eq!(SourceLink::parse(&parsed.to_url()), Ok(parsed));
            }
        }
    }
}
