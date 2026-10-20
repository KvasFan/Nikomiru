// SPDX-License-Identifier: GPL-3.0-only

//! HTML escaping for card fields: every value from a subtitle, a dictionary,
//! a file name or the user goes through it ([ADR 0007] §11).
//!
//! [ADR 0007]: ../../../docs/adr/0007-mining-to-anki.md

/// `text` with `& < > " '` escaped, for element content and attribute values.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// `text` escaped, its line breaks as `<br>`.
pub fn lines(text: &str) -> String {
    text.split('\n')
        .map(|line| escape(line.trim_end_matches('\r')))
        .collect::<Vec<_>>()
        .join("<br>")
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-ANK-021
    #[test]
    fn markup_becomes_text() {
        assert_eq!(
            escape(r#"<b onclick="x()">A & B's</b>"#),
            "&lt;b onclick=&quot;x()&quot;&gt;A &amp; B&#39;s&lt;/b&gt;"
        );
        assert_eq!(lines("一行\r\n<二>"), "一行<br>&lt;二&gt;");
    }
}
