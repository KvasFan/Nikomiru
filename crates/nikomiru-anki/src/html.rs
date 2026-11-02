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

/// The text of a card's HTML, for a card shown without a browser (REV-001):
/// tags dropped, a block or a line break as a new line, an item of a list
/// as a line of its own starting with "• ", styles and scripts left out,
/// entities decoded, empty lines dropped.
pub fn text(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    // inside <style> or <script>, until its end tag
    let mut skip: Option<&str> = None;
    while let Some(open) = rest.find('<') {
        if skip.is_none() {
            out.push_str(&decode(&rest[..open]));
        }
        let Some(close) = rest[open..].find('>') else {
            rest = "";
            break;
        };
        let tag = rest[open + 1..open + close].trim().to_ascii_lowercase();
        rest = &rest[open + close + 1..];
        let end = tag.starts_with('/');
        let name: String = tag
            .trim_start_matches('/')
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect();
        if let Some(until) = skip {
            if end && name == until {
                skip = None;
            }
            continue;
        }
        match name.as_str() {
            "style" | "script" if !end => {
                skip = Some(if name == "style" { "style" } else { "script" });
            }
            "li" if !end => out.push_str("\n• "),
            "br" | "div" | "p" | "ol" | "ul" | "li" | "tr" | "table" | "h1" | "h2" | "h3" => {
                out.push('\n');
            }
            "td" | "th" if end => out.push(' '),
            _ => {}
        }
    }
    if skip.is_none() {
        out.push_str(&decode(rest));
    }
    out.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && *l != "•")
        .collect::<Vec<_>>()
        .join("\n")
}

/// `text` with its character entities decoded.
fn decode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        let entity = rest[1..]
            .find(';')
            .filter(|&n| n <= 10)
            .map(|n| &rest[1..=n]);
        let decoded = entity.and_then(|e| match e {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some(' '),
            _ => {
                let number = e.strip_prefix('#')?;
                let code = match number.strip_prefix(['x', 'X']) {
                    Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                    None => number.parse().ok()?,
                };
                char::from_u32(code)
            }
        });
        match (entity, decoded) {
            (Some(e), Some(c)) => {
                out.push(c);
                rest = &rest[e.len() + 2..];
            }
            _ => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-REV-001
    #[test]
    fn a_glossary_becomes_lines_of_text() {
        let html = r#"<style>.x{color:red}</style><div style="text-align: left;" class="yomitan-glossary"><ol><li data-dictionary="JMdict"><i>(v1, JMdict)</i> <span>to eat</span></li><li><span>to live on &amp; by</span></li></ol></div>"#;
        assert_eq!(text(html), "• (v1, JMdict) to eat\n• to live on & by");
        assert_eq!(
            text("a<br>b&#x3042;&#12354;&unknown; &lt;c&gt;"),
            "a\nbああ&unknown; <c>"
        );
        assert_eq!(
            text("<table><tr><td>一</td><td>二</td></tr></table>"),
            "一 二"
        );
        assert_eq!(text("unclosed <b"), "unclosed");
        assert_eq!(text(&escape("<&'`$\\>")), "<&'`$\\>", "escape, then back");
    }

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
