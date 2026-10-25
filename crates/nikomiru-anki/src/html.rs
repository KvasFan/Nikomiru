// SPDX-License-Identifier: GPL-3.0-only

//! HTML escaping for card fields: every value from a subtitle, a dictionary,
//! a file name or the user goes through it ([ADR 0007] §11). Card templates
//! such as Lapis also paste fields into JavaScript template literals, so the
//! characters that end or open one there are escaped as well; the fields a
//! template reads through Anki's `text:` filter, which decodes entities again,
//! need [`script_text`].
//!
//! [ADR 0007]: ../../../docs/adr/0007-mining-to-anki.md

/// `text` with `& < > " '` escaped, for element content and attribute
/// values, and `` ` `` `$` `\` too: inside a template's script they would end
/// a template literal, open `${…}` or start an escape.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            '`' => out.push_str("&#96;"),
            '$' => out.push_str("&#36;"),
            '\\' => out.push_str("&#92;"),
            c => out.push(c),
        }
    }
    out
}

/// An escaped field value made safe for a template that reads it through
/// Anki's `text:` filter inside a script: the filter strips the tags and
/// decodes the entities, so what would come back as `<`, `>`, `` ` ``, `$` or
/// `\` becomes its fullwidth form instead (Lapis reads `＼` as a downstep,
/// like `\`).
pub fn script_text(escaped: &str) -> String {
    escaped
        .replace("&lt;", "＜")
        .replace("&gt;", "＞")
        .replace("&#96;", "｀")
        .replace("&#36;", "＄")
        .replace("&#92;", "＼")
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
        // nothing a script's template literal would read as its own
        assert_eq!(escape(r"`${x}\u"), "&#96;&#36;{x}&#92;u");
        assert_eq!(script_text(&escape(r"a`$\</script>")), "a｀＄＼＜/script＞");
    }
}
