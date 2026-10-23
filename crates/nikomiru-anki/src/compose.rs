// SPDX-License-Identifier: GPL-3.0-only

//! The field composer (ANK-020…ANK-027, ANK-053): the values of the
//! placeholders for one popup entry and its scene, and the templates of the
//! note type's fields filled with them. Every value is HTML; every text in it
//! is escaped.
//!
//! Translated from Fushi `packages/fushi_anki/lib/src/anki_models.dart`
//! (`renderTemplate`, the clip timestamp), `anki_note_composer.dart` (tags,
//! empty fields) and `fushi/assets/popup/popup.js` (`constructGlossaryHtml`,
//! `constructSingleGlossaryHtml`, `constructFuriganaPlain`,
//! `constructFrequencyHtml`, `constructPitchPositionHtml`,
//! `constructPitchCategories`), commit `ac8034e`. Unlike Fushi, every value is
//! escaped (ADR 0007 §11) and the part-of-speech labels start again with each
//! dictionary in `{glossary}` too.

use nikomiru_dict::css::Sheet;
use nikomiru_dict::frequency;
use nikomiru_dict::lookup::Entry;
use nikomiru_dict::pitch;
use nikomiru_dict::present;
use nikomiru_dict::structured::Image;
use nikomiru_dict::yomitan::PitchPosition;
use serde_json::Value;

use crate::html::escape;

/// Largest image of a glossary sent to Anki.
pub const MAX_IMAGE_BYTES: usize = 4 << 20;

/// Images of the glossaries sent with one card at most.
pub const MAX_IMAGES: usize = 32;

/// Part-of-speech tags whose repetition is not shown (Fushi `POS_TAGS`).
const POS: [&str; 12] = [
    "n", "adj-i", "adj-na", "adj-no", "v1", "vk", "vs", "vs-i", "vs-s", "vz", "vi", "vt",
];

fn is_pos(tag: &str) -> bool {
    POS.contains(&tag) || tag.starts_with("v5")
}

/// A dictionary of the lookup, as the composer needs it.
#[derive(Debug, Clone, Copy)]
pub struct Source<'a> {
    /// Its title (`index.json`): what the cards name, and what the Lapis CSS
    /// selects on.
    pub title: &'a str,
    /// The name shown in Nikomiru.
    pub name: &'a str,
    /// Its `styles.css` and the user's CSS (DIC-035).
    pub sheet: &'a Sheet,
}

/// The placeholders that come from the dictionaries (ANK-021…ANK-025), as HTML.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DictionaryFields {
    /// `{expression}`.
    pub expression: String,
    /// `{reading}`.
    pub reading: String,
    /// `{furigana-plain}`.
    pub furigana_plain: String,
    /// `{glossary}`.
    pub glossary: String,
    /// One glossary per dictionary, in order: title, name shown, HTML
    /// (`{glossary-first}`, `{single-glossary-<dictionary>}`).
    pub glossaries: Vec<(String, String, String)>,
    /// `{frequencies}`.
    pub frequencies: String,
    /// `{frequency-harmonic-rank}`.
    pub frequency_rank: String,
    /// `{pitch-accent-positions}`.
    pub pitch_positions: String,
    /// `{pitch-accent-categories}`.
    pub pitch_categories: String,
    /// `{phonetic-transcriptions}`.
    pub phonetic: String,
    /// `{tags}`: the term tags.
    pub tags: String,
    /// The images of the glossaries: Anki file name and bytes.
    pub media: Vec<(String, Vec<u8>)>,
}

/// The kind of an image file, from its first bytes: its extension. Besides
/// the types of POP-010, SVG and AVIF, which Anki shows (ADR 0007 §13).
pub fn image_extension(bytes: &[u8]) -> Option<&'static str> {
    let starts = |sig: &[u8]| bytes.starts_with(sig);
    if starts(b"\x89PNG\r\n\x1a\n") {
        Some("png")
    } else if starts(&[0xFF, 0xD8, 0xFF]) {
        Some("jpg")
    } else if starts(b"GIF87a") || starts(b"GIF89a") {
        Some("gif")
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("webp")
    } else if starts(b"BM") {
        Some("bmp")
    } else if bytes.len() >= 12
        && &bytes[4..8] == b"ftyp"
        && matches!(&bytes[8..12], b"avif" | b"avis")
    {
        Some("avif")
    } else {
        is_svg(bytes).then_some("svg")
    }
}

/// Whether `bytes` are an SVG document: after a byte order mark, an XML
/// declaration, comments and a doctype, the root element is `<svg>`.
fn is_svg(bytes: &[u8]) -> bool {
    fn past<'a>(bytes: &'a [u8], end: &[u8]) -> Option<&'a [u8]> {
        bytes
            .windows(end.len())
            .position(|w| w == end)
            .map(|i| &bytes[i + end.len()..])
    }
    let mut rest = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    loop {
        rest = rest.trim_ascii_start();
        let skipped = if let Some(after) = rest.strip_prefix(b"<?") {
            past(after, b"?>")
        } else if let Some(after) = rest.strip_prefix(b"<!--") {
            past(after, b"-->")
        } else if let Some(after) = rest.strip_prefix(b"<!") {
            past(after, b">")
        } else {
            let root = rest
                .get(..4)
                .is_some_and(|t| t.eq_ignore_ascii_case(b"<svg"));
            let end = rest
                .get(4)
                .is_some_and(|c| c.is_ascii_whitespace() || matches!(c, b'>' | b'/'));
            return root && end;
        };
        match skipped {
            Some(after) => rest = after,
            None => return false,
        }
    }
}

/// `{furigana-plain}` (ANK-022): `食[た]べ 物[もの]`.
pub fn furigana_plain(expression: &str, reading: &str) -> String {
    let mut out = String::new();
    for s in nikomiru_text::furigana::distribute(expression, reading) {
        if s.reading.is_empty() {
            out.push_str(&s.text);
            out.push(' ');
        } else {
            out.push_str(&s.text);
            out.push('[');
            out.push_str(&s.reading);
            out.push(']');
        }
    }
    escape(out.trim())
}

/// The label of a sense (ANK-023): `(tags, D)` on a dictionary's first sense,
/// `(tags)` after it; part-of-speech tags equal to the previous sense's are
/// left out, numbers always.
fn label(
    definition_tags: &str,
    first: bool,
    title: &str,
    previous_pos: &mut Option<Vec<String>>,
) -> String {
    let tags: Vec<&str> = definition_tags
        .split_whitespace()
        .filter(|t| !t.chars().all(|c| c.is_ascii_digit()))
        .collect();
    let mut pos: Vec<String> = tags
        .iter()
        .filter(|t| is_pos(t))
        .map(|t| t.to_string())
        .collect();
    pos.sort();
    pos.dedup();
    let repeated = previous_pos.as_ref() == Some(&pos);
    let shown: Vec<String> = tags
        .iter()
        .filter(|t| !(repeated && is_pos(t)))
        .map(|t| escape(t))
        .collect();
    *previous_pos = Some(pos);
    let tags = shown.join(", ");
    match (first, tags.is_empty()) {
        (true, true) => format!("({})", escape(title)),
        (true, false) => format!("({tags}, {})", escape(title)),
        (false, true) => String::new(),
        (false, false) => format!("({tags})"),
    }
}

/// The glossary block of ANK-023 around `items`, with the scoped CSS of the
/// dictionaries in `styles`.
fn glossary_block(items: &str, styles: &[(&str, &Sheet)]) -> String {
    let mut out =
        format!(r#"<div style="text-align: left;" class="yomitan-glossary"><ol>{items}</ol>"#);
    for (title, sheet) in styles {
        let scope = format!(
            r#".yomitan-glossary [data-dictionary={}]"#,
            nikomiru_dict::style::css_string(title)
        );
        let css = sheet.to_css_used(&scope, items);
        if !css.is_empty() {
            out.push_str("<style>");
            out.push_str(css.trim_end());
            out.push_str("</style>");
        }
    }
    out.push_str("</div>");
    out
}

/// The dictionary placeholders of `entry` (ANK-021…ANK-025). `sources[i]`
/// describes the dictionary of index `i`; `media(i, path)` reads a file of
/// dictionary `i`, for the images of its glossaries.
pub fn dictionary_fields(
    entry: &Entry,
    sources: &[Source<'_>],
    media: &mut dyn FnMut(usize, &str) -> Option<Vec<u8>>,
) -> DictionaryFields {
    let source = |i: usize| sources.get(i).copied();
    let title = |i: usize| source(i).map(|s| s.title.to_string()).unwrap_or_default();
    let mut images: Vec<(String, Vec<u8>)> = Vec::new();

    // senses, grouped by dictionary in their order
    let mut all = String::new();
    let mut singles: Vec<(usize, String)> = Vec::new();
    let mut previous_pos: Option<Vec<String>> = None;
    let mut term_tags: Vec<String> = Vec::new();
    for g in &entry.glossaries {
        let Some(src) = source(g.dictionary) else {
            continue;
        };
        let Ok(text) = g.term.glossary() else {
            continue;
        };
        let Ok(json) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        if present::is_redirect_label(&g.term.definition_tags, &g.term.term_tags, &json) {
            continue;
        }
        for t in g.term.term_tags.split_whitespace() {
            if !term_tags.iter().any(|s| s == t) {
                term_tags.push(t.to_string());
            }
        }
        let first = singles.last().is_none_or(|(d, _)| *d != g.dictionary);
        if first {
            singles.push((g.dictionary, String::new()));
            previous_pos = None;
        }
        let label = label(&g.term.definition_tags, first, src.title, &mut previous_pos);
        let dictionary = g.dictionary;
        let content = crate::glossary::definitions(&json, &mut |image: &Image| {
            attach(&mut images, media(dictionary, &image.path))
        });
        let item = format!(
            r#"<li data-dictionary="{}"><i>{label}</i> <span>{content}</span></li>"#,
            escape(src.title)
        );
        all.push_str(&item);
        if let Some((_, items)) = singles.last_mut() {
            items.push_str(&item);
        }
    }
    let mut used: Vec<usize> = Vec::new();
    for (d, _) in &singles {
        if !used.contains(d) {
            used.push(*d);
        }
    }
    let styles: Vec<(&str, &Sheet)> = used
        .iter()
        .filter_map(|&d| source(d).map(|s| (s.title, s.sheet)))
        .collect();
    let glossary = if all.is_empty() {
        String::new()
    } else {
        glossary_block(&all, &styles)
    };
    // one block per dictionary, merging its runs if it appears twice
    let mut glossaries: Vec<(String, String, String)> = Vec::new();
    for &d in &used {
        let items: String = singles
            .iter()
            .filter(|(i, _)| *i == d)
            .map(|(_, items)| items.as_str())
            .collect();
        let Some(src) = source(d) else { continue };
        glossaries.push((
            src.title.to_string(),
            src.name.to_string(),
            glossary_block(&items, &[(src.title, src.sheet)]),
        ));
    }

    // frequencies (ANK-025)
    let groups: Vec<(String, &[nikomiru_dict::yomitan::Frequency])> = entry
        .frequencies
        .iter()
        .map(|f| (title(f.dictionary), f.values.as_slice()))
        .collect();
    let items: String = groups
        .iter()
        .flat_map(|(name, values)| {
            values.iter().map(move |v| {
                format!(
                    "<li>{}: {}</li>",
                    escape(name),
                    escape(&frequency::shown(v))
                )
            })
        })
        .collect();
    let frequencies = if items.is_empty() {
        String::new()
    } else {
        format!(r#"<ul style="text-align: left;">{items}</ul>"#)
    };
    let frequency_rank =
        frequency::harmonic_rank(groups.iter().map(|(n, v)| (n.as_str(), *v))).to_string();

    // pitch accent (Fushi merges identical groups for the positions only)
    let pitch_groups: Vec<pitch::Group> = entry
        .pitches
        .iter()
        .map(|p| pitch::Group {
            dictionary: title(p.dictionary),
            positions: p
                .pitches
                .iter()
                .filter_map(|a| match a.position {
                    PitchPosition::Downstep(n) => Some(n),
                    PitchPosition::Pattern(_) => None,
                })
                .collect(),
            patterns: p
                .pitches
                .iter()
                .filter_map(|a| match &a.position {
                    PitchPosition::Pattern(s) => Some(s.clone()),
                    PitchPosition::Downstep(_) => None,
                })
                .collect(),
            transcriptions: p.ipa.iter().map(|t| t.ipa.clone()).collect(),
        })
        .collect();
    let rows = pitch::rows(&pitch_groups, false);
    let item = |x: &str| {
        format!(
            r#"<li><span style="display:inline;"><span>[</span><span>{}</span><span>]</span></span></li>"#,
            escape(x)
        )
    };
    let list = |items: String| {
        if items.is_empty() {
            String::new()
        } else {
            format!("<ul>{items}</ul>")
        }
    };
    let pitch_positions = list(
        rows.iter()
            .flat_map(|r| {
                r.positions
                    .iter()
                    .map(u32::to_string)
                    .chain(r.patterns.iter().cloned())
                    .chain(r.transcriptions.iter().cloned())
                    .collect::<Vec<_>>()
            })
            .map(|x| item(&x))
            .collect(),
    );
    let phonetic = list(
        rows.iter()
            .flat_map(|r| r.transcriptions.iter())
            .map(|x| item(x))
            .collect(),
    );
    let morae = pitch::morae(&entry.reading).len();
    let mut categories: Vec<&str> = Vec::new();
    for g in &pitch_groups {
        for &p in &g.positions {
            let c = pitch::category(morae, p, &entry.rules).name();
            if !categories.contains(&c) {
                categories.push(c);
            }
        }
    }

    DictionaryFields {
        expression: escape(&entry.expression),
        reading: escape(&entry.reading),
        furigana_plain: furigana_plain(&entry.expression, &entry.reading),
        glossary,
        glossaries,
        frequencies,
        frequency_rank,
        pitch_positions,
        pitch_categories: categories.join(","),
        phonetic,
        tags: term_tags
            .iter()
            .map(|t| escape(t))
            .collect::<Vec<_>>()
            .join(", "),
        media: images,
    }
}

/// The Anki name of an image of a glossary, which joins `images` if it is new;
/// `None` for what is not a usable image.
fn attach(images: &mut Vec<(String, Vec<u8>)>, bytes: Option<Vec<u8>>) -> Option<String> {
    let bytes = bytes.filter(|b| b.len() <= MAX_IMAGE_BYTES)?;
    let ext = image_extension(&bytes)?;
    let name = crate::media::name(&bytes, ext);
    if !images.iter().any(|(n, _)| *n == name) {
        if images.len() >= MAX_IMAGES {
            return None;
        }
        images.push((name.clone(), bytes));
    }
    Some(name)
}

/// The placeholders that come from the scene and the media (ANK-024,
/// ANK-050, ANK-053), as HTML.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SceneFields {
    /// `{sentence}`.
    pub sentence: String,
    /// `{cue-sentence}`.
    pub cue_sentence: String,
    /// `{sentence-audio}`: `[sound:…]`.
    pub sentence_audio: String,
    /// `{card-image}`: `<img src="…">`.
    pub card_image: String,
    /// `{audio}`: word audio (M5).
    pub audio: String,
    /// `{document-title}`.
    pub document_title: String,
    /// `{source-link}`.
    pub source_link: String,
    /// `{clip-timestamp}`.
    pub clip_timestamp: String,
}

/// `{sentence}` (ANK-024): the text escaped, lines joined by `<br>`, and the
/// looked-up `word` in `<b>` at byte `offset`, or at its first occurrence when
/// the offset does not hold it.
pub fn sentence(text: &str, offset: Option<usize>, word: &str) -> String {
    let span = (!word.is_empty())
        .then(|| {
            offset
                .filter(|&o| text.get(o..).is_some_and(|rest| rest.starts_with(word)))
                .or_else(|| text.find(word))
        })
        .flatten()
        .map(|o| (o, o + word.len()));
    match span {
        Some((start, end)) => format!(
            "{}<b>{}</b>{}",
            crate::html::lines(&text[..start]),
            crate::html::lines(&text[start..end]),
            crate::html::lines(&text[end..])
        ),
        None => crate::html::lines(text),
    }
}

/// `{clip-timestamp}` (ANK-053): `HH:MM:SS - HH:MM:SS`, seconds truncated,
/// empty unless `end > start`.
pub fn clip_timestamp(start_ms: i64, end_ms: i64) -> String {
    if end_ms <= start_ms {
        return String::new();
    }
    let clock = |ms: i64| {
        let s = ms.max(0) / 1000;
        format!("{:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    };
    format!("{} - {}", clock(start_ms), clock(end_ms))
}

/// The tags of a card (ANK-027): the user's, then `nikomiru` and `video`,
/// without repeats, in order. The series tag waits for series (M5).
/// Templates paste the tags into scripts unescaped (Lapis' `{{Tags}}`), so the
/// characters that would end a literal or a script there are left out.
pub fn tags(user: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for t in user
        .iter()
        .flat_map(|t| t.split_whitespace())
        .map(|t| {
            t.chars()
                .filter(|c| !"`$\\<>".contains(*c))
                .collect::<String>()
        })
        .chain(["nikomiru".to_string(), "video".to_string()])
    {
        if !t.is_empty() && !out.contains(&t) {
            out.push(t);
        }
    }
    out
}

/// The field values of a Lapis-like note type made safe for its scripts: the
/// fields its templates read through `text:` inside a script
/// ([`crate::lapis::SCRIPT_TEXT_FIELDS`]) get [`crate::html::script_text`].
pub fn lapis_safe(fields: Vec<(String, String)>) -> Vec<(String, String)> {
    fields
        .into_iter()
        .map(|(name, value)| {
            if crate::lapis::SCRIPT_TEXT_FIELDS.contains(&name.as_str()) {
                let safe = crate::html::script_text(&value);
                (name, safe)
            } else {
                (name, value)
            }
        })
        .collect()
}

/// The value of placeholder `name` (without braces); unknown → empty.
pub fn value(name: &str, dict: &DictionaryFields, scene: &SceneFields) -> String {
    let single = |key: &str| {
        dict.glossaries
            .iter()
            .find(|(title, _, _)| title == key)
            .or_else(|| dict.glossaries.iter().find(|(_, shown, _)| shown == key))
            .map(|(_, _, html)| html.clone())
            .unwrap_or_default()
    };
    match name {
        "expression" => dict.expression.clone(),
        "reading" => dict.reading.clone(),
        "furigana-plain" => dict.furigana_plain.clone(),
        "glossary" => dict.glossary.clone(),
        "glossary-first" => dict
            .glossaries
            .first()
            .map(|(_, _, html)| html.clone())
            .unwrap_or_default(),
        "frequencies" => dict.frequencies.clone(),
        "frequency-harmonic-rank" => dict.frequency_rank.clone(),
        "pitch-accent-positions" => dict.pitch_positions.clone(),
        "pitch-accent-categories" => dict.pitch_categories.clone(),
        "phonetic-transcriptions" => dict.phonetic.clone(),
        "tags" => dict.tags.clone(),
        "sentence" => scene.sentence.clone(),
        "cue-sentence" => scene.cue_sentence.clone(),
        "sentence-audio" => scene.sentence_audio.clone(),
        "card-image" => scene.card_image.clone(),
        "audio" => scene.audio.clone(),
        "document-title" => scene.document_title.clone(),
        "source-link" => scene.source_link.clone(),
        "clip-timestamp" => scene.clip_timestamp.clone(),
        other => other
            .strip_prefix("single-glossary-")
            .map(single)
            .unwrap_or_default(),
    }
}

/// A field template with its `{placeholders}` filled (ANK-020): from each
/// `{` to the next `}` is a placeholder, as the regex `\{[^}]*\}` of Fushi
/// finds them; an unknown one becomes empty, the rest stays as written; the
/// result is trimmed.
pub fn render(template: &str, value: &dyn Fn(&str) -> String) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else {
            break;
        };
        out.push_str(&rest[..open]);
        out.push_str(&value(&after[..close]));
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    out.trim().to_string()
}

/// Why the fields of a note cannot be sent (ANK-026).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldsError {
    /// No field of the mapping is a field of the note type.
    NoMappedField,
    /// The note type's first field is empty.
    EmptyFirstField,
}

/// The fields of a note (ANK-026): the mapping's templates filled, only for
/// fields of the note type, in its order. A new note leaves out the empty ones
/// (`keep_empty` false); an overwrite sends every field of the note type,
/// unmapped ones empty (ANK-037).
pub fn fields(
    note_fields: &[String],
    mapping: &[(String, String)],
    value: &dyn Fn(&str) -> String,
    keep_empty: bool,
) -> Result<Vec<(String, String)>, FieldsError> {
    if !mapping.iter().any(|(f, _)| note_fields.contains(f)) {
        return Err(FieldsError::NoMappedField);
    }
    let mut out = Vec::new();
    for (i, field) in note_fields.iter().enumerate() {
        let text = mapping
            .iter()
            .find(|(f, _)| f == field)
            .map(|(_, template)| render(template, value))
            .unwrap_or_default();
        if i == 0 && text.is_empty() {
            return Err(FieldsError::EmptyFirstField);
        }
        if keep_empty || !text.is_empty() {
            out.push((field.clone(), text));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-ANK-023: the label names a dictionary on its first sense only, and
    // leaves out a part of speech the sense before had
    #[test]
    fn labels_name_each_dictionary_once_and_skip_a_repeated_part_of_speech() {
        use nikomiru_dict::lookup::Glossary;
        use nikomiru_dict::store::StoredTerm;
        use nikomiru_dict::yomitan::Term;
        let sense = |dictionary, tags: &str, gloss: &str| Glossary {
            dictionary,
            term: StoredTerm::new(
                0,
                Term {
                    expression: "見る".into(),
                    reading: "みる".into(),
                    definition_tags: tags.into(),
                    rules: "v1".into(),
                    score: 0.0,
                    glossary: serde_json::to_string(&[gloss]).unwrap(),
                    sequence: 0,
                    term_tags: String::new(),
                },
            ),
        };
        let entry = Entry {
            expression: "見る".into(),
            reading: "みる".into(),
            matched: "見る".into(),
            deinflected: "見る".into(),
            trace: Vec::new(),
            preprocessor_steps: 0,
            rules: "v1".into(),
            score: 0.0,
            glossaries: vec![
                sense(0, "v1 vt 1", "to see"),
                sense(0, "vt v1 P", "to look"),
                sense(0, "v1 vt", "to watch"),
                sense(0, "n", "a look"),
                sense(1, "v1 vt", "to view"),
            ],
            frequencies: Vec::new(),
            pitches: Vec::new(),
        };
        let sheet = Sheet::parse("");
        let sources = [
            Source {
                title: "Mini",
                name: "mini",
                sheet: &sheet,
            },
            Source {
                title: "Other <b>",
                name: "other",
                sheet: &sheet,
            },
        ];
        let fields = dictionary_fields(&entry, &sources, &mut |_, _| None);
        let labels: Vec<&str> = fields
            .glossary
            .split("<i>")
            .skip(1)
            .filter_map(|s| s.split("</i>").next())
            .collect();
        assert_eq!(
            labels,
            [
                "(v1, vt, Mini)",
                "(P)",
                "",
                "(n)",
                "(v1, vt, Other &lt;b&gt;)"
            ],
            "{}",
            fields.glossary
        );
    }

    // T-ANK-022: Fushi popup.js `constructFuriganaPlain`, trimmed
    #[test]
    fn furigana_plain_vectors() {
        for (expression, reading, plain) in [
            ("食べ物", "たべもの", "食[た]べ 物[もの]"),
            ("食べる", "たべる", "食[た]べる"),
            ("お茶", "おちゃ", "お 茶[ちゃ]"),
            ("取り扱い", "とりあつかい", "取[と]り 扱[あつか]い"),
            ("お父さん", "おとうさん", "お 父[とう]さん"),
            ("大人しい", "おとなしい", "大人[おとな]しい"),
            ("勉強", "べんきょう", "勉強[べんきょう]"),
            ("カタカナ", "かたかな", "カタカナ[かたかな]"),
            ("ひらがな", "ひらがな", "ひらがな"),
            ("今日", "", "今日"),
            ("時々", "ときどき", "時々[ときどき]"),
        ] {
            assert_eq!(furigana_plain(expression, reading), plain, "{expression}");
        }
    }

    // T-ANK-024
    #[test]
    fn the_sentence_bolds_the_word_where_it_was_clicked() {
        let text = "食べ物と食べ物";
        let second = text.rfind("食べ物").unwrap();
        assert_eq!(
            sentence(text, Some(second), "食べ物"),
            "食べ物と<b>食べ物</b>"
        );
        // an offset that is not a character boundary: the first occurrence
        assert_eq!(sentence(text, Some(1), "食べ物"), "<b>食べ物</b>と食べ物");
        // an offset that holds another word: the first occurrence
        let other = "飲み物と食べ物";
        assert_eq!(sentence(other, Some(0), "食べ物"), "飲み物と<b>食べ物</b>");
        assert_eq!(
            sentence(other, Some(100), "食べ物"),
            "飲み物と<b>食べ物</b>"
        );
        assert_eq!(sentence(text, None, "食べ物"), "<b>食べ物</b>と食べ物");
        assert_eq!(sentence(text, None, "飲み物"), "食べ物と食べ物");
        assert_eq!(sentence(text, Some(0), ""), "食べ物と食べ物");
        // lines and markup in the cue
        assert_eq!(
            sentence("<i>a</i>\nb", None, "b"),
            "&lt;i&gt;a&lt;/i&gt;<br><b>b</b>"
        );
    }

    // T-ANK-053: Fushi `handlebar_clip_timestamp_test.dart`
    #[test]
    fn clip_timestamps() {
        assert_eq!(clip_timestamp(754_000, 758_000), "00:12:34 - 00:12:38");
        assert_eq!(clip_timestamp(3_723_000, 3_729_000), "01:02:03 - 01:02:09");
        assert_eq!(
            clip_timestamp(36_000_000, 36_061_000),
            "10:00:00 - 10:01:01"
        );
        assert_eq!(clip_timestamp(1_999, 2_999), "00:00:01 - 00:00:02");
        assert_eq!(clip_timestamp(0, 4_000), "00:00:00 - 00:00:04");
        assert_eq!(clip_timestamp(0, 0), "");
        assert_eq!(clip_timestamp(5_000, 4_000), "");
    }

    // T-ANK-027
    #[test]
    fn tags_are_the_users_then_nikomiru_and_video() {
        assert_eq!(tags(&[]), ["nikomiru", "video"]);
        assert_eq!(
            tags(&["mine nikomiru".into(), "jp::vocab".into(), "mine".into()]),
            ["mine", "nikomiru", "jp::vocab", "video"]
        );
        // nothing that would end the script Lapis pastes them into
        assert_eq!(
            tags(&["a`b ${c} </script> `".into()]),
            ["ab", "{c}", "/script", "nikomiru", "video"]
        );
    }

    // T-ANK-020
    #[test]
    fn templates() {
        let value = |name: &str| match name {
            "expression" => "語".to_string(),
            "source-link" => "<a>s</a>".to_string(),
            _ => String::new(),
        };
        assert_eq!(render("{expression}", &value), "語");
        assert_eq!(
            render(" {source-link} {clip-timestamp} ", &value),
            "<a>s</a>"
        );
        assert_eq!(render("x", &value), "x");
        assert_eq!(render("{unknown}{expression}", &value), "語");
        // from a `{` to the next `}`, as Fushi's regex
        assert_eq!(render("a { b {expression} }", &value), "a  }");
        assert_eq!(render("{{expression}}", &value), "}");
        assert_eq!(render("{expression} {", &value), "語 {");
        assert_eq!(render("}{expression}", &value), "}語");
        assert_eq!(render("{", &value), "{");
    }

    // T-ANK-026
    #[test]
    fn fields_follow_the_note_type() {
        let note: Vec<String> = ["Word", "Meaning", "Extra"].map(String::from).to_vec();
        let mapping: Vec<(String, String)> = [
            ("Word", "{expression}"),
            ("Meaning", "{glossary}"),
            ("Missing", "{expression}"),
        ]
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .to_vec();
        let value = |name: &str| {
            if name == "expression" {
                "語".into()
            } else {
                String::new()
            }
        };
        assert_eq!(
            fields(&note, &mapping, &value, false),
            Ok(vec![("Word".to_string(), "語".to_string())])
        );
        assert_eq!(
            fields(&note, &mapping, &value, true).unwrap(),
            [
                ("Word".to_string(), "語".to_string()),
                ("Meaning".to_string(), String::new()),
                ("Extra".to_string(), String::new()),
            ]
        );
        let empty = |_: &str| String::new();
        assert_eq!(
            fields(&note, &mapping, &empty, false),
            Err(FieldsError::EmptyFirstField)
        );
        let other: Vec<String> = ["Front"].map(String::from).to_vec();
        assert_eq!(
            fields(&other, &mapping, &value, false),
            Err(FieldsError::NoMappedField)
        );
    }

    #[test]
    fn image_kinds() {
        assert_eq!(image_extension(b"\x89PNG\r\n\x1a\nrest"), Some("png"));
        assert_eq!(image_extension(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("jpg"));
        assert_eq!(image_extension(b"RIFF\0\0\0\0WEBPVP8 "), Some("webp"));
        assert_eq!(
            image_extension(b"<?xml version=\"1.0\"?><svg/>"),
            Some("svg")
        );
        assert_eq!(
            image_extension(
                b"\xEF\xBB\xBF<?xml version=\"1.0\"?>\n<!-- gaiji -->\n<!DOCTYPE svg PUBLIC \"-//W3C//DTD SVG 1.1//EN\" \"x.dtd\">\n<SVG xmlns=\"http://www.w3.org/2000/svg\"></SVG>"
            ),
            Some("svg")
        );
        assert_eq!(image_extension(b"\0\0\0\x1cftypavif\0\0\0\0"), Some("avif"));
        // the root element decides: an HTML page with an <svg> inside is not one
        for not_svg in [
            &b"<html><body><svg/></body></html>"[..],
            b"<svgx/>",
            b"<svg",
            b"<?xml version=\"1.0\"?",
            b"text <svg/>",
        ] {
            assert_eq!(image_extension(not_svg), None, "{not_svg:?}");
        }
        assert_eq!(image_extension(b"MZ\x90\0"), None);
    }
}
