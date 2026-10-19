// SPDX-License-Identifier: GPL-3.0-only

//! The content of a dictionary popup as JSON for QML (POP-007, POP-008,
//! POP-013): headword with furigana, trace tags, frequency and pitch rows,
//! tags, and one block per dictionary with its glossaries already converted
//! to the safe view model of [`nikomiru_dict::structured`].

use nikomiru_dict::css::Sheet;
use nikomiru_dict::lookup::{Entry, KanjiEntry};
use nikomiru_dict::yomitan::{PitchPosition, Tag};
use nikomiru_dict::{frequency, pitch, present, structured};
use serde_json::{Value, json};

/// How a dictionary block starts (DIC-034).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Collapse {
    /// Expanded among the first [`AUTOMATIC_EXPANDED`] blocks.
    #[default]
    Automatic,
    /// Expanded.
    Expanded,
    /// Collapsed.
    Collapsed,
}

/// Blocks expanded by [`Collapse::Automatic`].
pub const AUTOMATIC_EXPANDED: usize = 3;

/// What the popup needs to know of a dictionary, by its index in the lookup.
#[derive(Debug, Clone, Default)]
pub struct DictView {
    /// Name shown.
    pub name: String,
    /// Block state.
    pub collapse: Collapse,
    /// Its styles (DIC-035): `styles.css` and the user's CSS.
    pub sheet: Sheet,
}

/// Looks a tag up in a dictionary's tag bank.
pub type TagLookup<'a> = &'a dyn Fn(usize, &str) -> Option<Tag>;

/// Reads a media file of a dictionary (its `media` table).
pub type MediaLookup<'a> = &'a dyn Fn(usize, &str) -> Option<Vec<u8>>;

/// Images larger than this are not shown inline.
pub const MAX_IMAGE_BYTES: usize = 1 << 20;

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(chunk.get(1).copied().unwrap_or(0)) << 8)
            | u32::from(chunk.get(2).copied().unwrap_or(0));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// The image type of a media path, for the images QML can show.
fn mime(path: &str) -> Option<&'static str> {
    let ext = path.rsplit('.').next()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        "avif" => "image/avif",
        _ => return None,
    })
}

/// Gives the image runs of rendered glossaries their pixels (POP-010): a
/// `data:` URL made from the dictionary's media table, so QML never loads an
/// address from a dictionary. Missing, unknown or too large images keep only
/// their alternative text.
fn attach_images(value: &mut Value, dictionary: usize, media: MediaLookup<'_>) {
    match value {
        Value::Object(o) => {
            if o.get("k").and_then(Value::as_str) == Some("image")
                && let Some(Value::Object(image)) = o.get_mut("image")
            {
                let path = image
                    .get("path")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let src = mime(&path).and_then(|m| {
                    let bytes = media(dictionary, &path).filter(|b| b.len() <= MAX_IMAGE_BYTES)?;
                    Some(format!("data:{m};base64,{}", base64(&bytes)))
                });
                if let Some(src) = src {
                    image.insert("src".into(), Value::String(src));
                }
                return;
            }
            for v in o.values_mut() {
                attach_images(v, dictionary, media);
            }
        }
        Value::Array(a) => {
            for v in a {
                attach_images(v, dictionary, media);
            }
        }
        _ => {}
    }
}

fn tag_chip(dictionary: usize, name: &str, tags: TagLookup<'_>) -> Value {
    match tags(dictionary, name) {
        Some(t) => json!({"name": name, "category": t.category, "notes": t.notes}),
        None => json!({"name": name, "category": "", "notes": ""}),
    }
}

/// Definition tags shown (POP-008): numbers hidden, and a part-of-speech tag
/// already shown on the previous sense of the same dictionary omitted.
fn definition_chips(
    dictionary: usize,
    names: &str,
    previous_pos: &mut Vec<String>,
    tags: TagLookup<'_>,
) -> Vec<Value> {
    let mut chips = Vec::new();
    let mut pos_now = Vec::new();
    for name in names.split_whitespace() {
        if name.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let info = tags(dictionary, name);
        let is_pos = info.as_ref().is_some_and(|t| t.category == "partOfSpeech");
        if is_pos {
            pos_now.push(name.to_string());
            if previous_pos.iter().any(|p| p == name) {
                continue;
            }
        }
        chips.push(tag_chip(dictionary, name, tags));
    }
    *previous_pos = pos_now;
    chips
}

fn entry_json(
    entry: &Entry,
    dicts: &[DictView],
    tags: TagLookup<'_>,
    media: MediaLookup<'_>,
) -> Value {
    let name = |i: usize| dicts.get(i).map(|d| d.name.clone()).unwrap_or_default();
    let furigana: Vec<Value> =
        nikomiru_text::furigana::distribute(&entry.expression, &entry.reading)
            .iter()
            .map(|s| json!({"t": s.text, "r": s.reading}))
            .collect();
    let trace: Vec<Value> = present::trace_tags(entry)
        .iter()
        .map(|s| json!({"name": &*s.name, "description": &*s.description}))
        .collect();
    let frequencies: Vec<Value> = entry
        .frequencies
        .iter()
        .map(|f| json!({"dictionary": name(f.dictionary), "values": f.values.iter().map(frequency::shown).collect::<Vec<_>>()}))
        .collect();
    let groups: Vec<pitch::Group> = entry
        .pitches
        .iter()
        .map(|p| pitch::Group {
            dictionary: name(p.dictionary),
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
    let morae = pitch::morae(&entry.reading);
    let pitches: Vec<Value> = pitch::rows(&groups, true)
        .iter()
        .map(|r| {
            let shapes: Vec<Value> = r
                .positions
                .iter()
                .map(|&p| {
                    json!({
                        "position": p,
                        "highLow": pitch::high_low(morae.len(), p),
                        "category": pitch::category(morae.len(), p, &entry.rules).name(),
                    })
                })
                .collect();
            json!({
                "dictionaries": r.dictionaries,
                "accents": shapes,
                "patterns": r.patterns,
                "transcriptions": r.transcriptions,
            })
        })
        .collect();

    // one block per dictionary, in the order of the glossaries
    let mut blocks: Vec<(usize, Vec<Value>, Vec<String>)> = Vec::new();
    let mut term_tags: Vec<Value> = Vec::new();
    let mut seen_term_tags: Vec<String> = Vec::new();
    for g in &entry.glossaries {
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
            if !seen_term_tags.iter().any(|s| s == t) {
                seen_term_tags.push(t.to_string());
                term_tags.push(tag_chip(g.dictionary, t, tags));
            }
        }
        let sheet = dicts
            .get(g.dictionary)
            .map(|d| &d.sheet)
            .cloned()
            .unwrap_or_default();
        if blocks.last().is_none_or(|(d, _, _)| *d != g.dictionary) {
            blocks.push((g.dictionary, Vec::new(), Vec::new()));
        }
        if let Some((d, glossaries, previous_pos)) = blocks.last_mut() {
            let chips = definition_chips(*d, &g.term.definition_tags, previous_pos, tags);
            let mut rendered =
                serde_json::to_value(structured::render(&json, &sheet)).unwrap_or_default();
            attach_images(&mut rendered, *d, media);
            glossaries.push(json!({
                "tags": chips,
                "blocks": rendered,
            }));
        }
    }
    let blocks: Vec<Value> = blocks
        .into_iter()
        .enumerate()
        .map(|(n, (d, glossaries, _))| {
            let view = dicts.get(d).cloned().unwrap_or_default();
            let expanded = match view.collapse {
                Collapse::Expanded => true,
                Collapse::Collapsed => false,
                Collapse::Automatic => n < AUTOMATIC_EXPANDED,
            };
            json!({"dictionary": view.name, "expanded": expanded, "glossaries": glossaries})
        })
        .collect();
    json!({
        "expression": entry.expression,
        "reading": entry.reading,
        "matched": entry.matched,
        "furigana": furigana,
        "morae": morae,
        "trace": trace,
        "termTags": term_tags,
        "frequencies": frequencies,
        "pitches": pitches,
        "blocks": blocks,
    })
}

fn kanji_json(k: &KanjiEntry, dicts: &[DictView]) -> Value {
    let name = |i: usize| dicts.get(i).map(|d| d.name.clone()).unwrap_or_default();
    let stat = |n: &str| {
        k.kanji
            .stats
            .iter()
            .find(|(s, _)| s == n)
            .map(|(_, v)| v.clone())
    };
    json!({
        "dictionary": name(k.dictionary),
        "character": k.kanji.character,
        "onyomi": k.kanji.onyomi.split_whitespace().collect::<Vec<_>>(),
        "kunyomi": k.kanji.kunyomi.split_whitespace().collect::<Vec<_>>(),
        "meanings": k.kanji.meanings,
        "strokes": stat("strokes"),
        "radical": stat("radical"),
        "stats": k.kanji.stats.iter().filter(|(n, _)| n != "strokes" && n != "radical").map(|(n, v)| json!([n, v])).collect::<Vec<_>>(),
        "frequencies": k.frequencies.iter().map(|f| json!({"dictionary": name(f.dictionary), "values": f.values.iter().map(frequency::shown).collect::<Vec<_>>()})).collect::<Vec<_>>(),
    })
}

/// The popup content for these entries and kanji entries.
pub fn content(
    entries: &[Entry],
    kanji: &[KanjiEntry],
    dicts: &[DictView],
    tags: TagLookup<'_>,
    media: MediaLookup<'_>,
) -> Value {
    json!({
        "entries": entries.iter().map(|e| entry_json(e, dicts, tags, media)).collect::<Vec<_>>(),
        "kanji": kanji.iter().map(|k| kanji_json(k, dicts)).collect::<Vec<_>>(),
        "bestLength": nikomiru_dict::lookup::best_length(entries),
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use nikomiru_dict::lookup::{Dictionary, Lookup, Options};
    use nikomiru_dict::memory::MemStore;
    use nikomiru_dict::yomitan::{Frequency, Meta, Pitch, Term};

    use super::*;

    fn term(expression: &str, reading: &str, tags: &str, gloss: &str) -> Term {
        Term {
            expression: expression.into(),
            reading: reading.into(),
            definition_tags: tags.into(),
            rules: "v1".into(),
            score: 0.0,
            glossary: gloss.into(),
            sequence: 0,
            term_tags: "P".into(),
        }
    }

    fn tag(name: &str, category: &str) -> Tag {
        Tag {
            name: name.into(),
            category: category.into(),
            order: 0,
            notes: format!("{name} notes"),
            score: 0.0,
        }
    }

    // T-POP-007, T-POP-008
    #[test]
    fn header_rows_and_blocks() {
        let store = MemStore::new("JM")
            .with_terms(vec![
                term("食べる", "たべる", "v1 vt 1", r#"["to eat"]"#),
                term("食べる", "たべる", "v1 vt 2", r#"["to live on"]"#),
            ])
            .with_meta(vec![
                (
                    "食べる".into(),
                    Meta::Frequency(Frequency {
                        reading: None,
                        value: 120,
                        display: None,
                    }),
                ),
                (
                    "食べる".into(),
                    Meta::Pitch {
                        reading: "たべる".into(),
                        pitches: vec![Pitch {
                            position: PitchPosition::Downstep(2),
                            nasal: vec![],
                            devoice: vec![],
                            tags: vec![],
                        }],
                    },
                ),
            ])
            .with_tags(vec![
                tag("v1", "partOfSpeech"),
                tag("vt", "partOfSpeech"),
                tag("P", "popular"),
            ]);
        let store = Arc::new(store);
        let lookup = Lookup::new(
            vec![Dictionary {
                id: "jm".into(),
                name: "JM".into(),
                store: store.clone(),
            }],
            "ja",
        );
        let entries = lookup.lookup("食べました", Options::default()).unwrap();
        let dicts = [DictView {
            name: "JM".into(),
            ..DictView::default()
        }];
        let tags = |_: usize, name: &str| {
            use nikomiru_dict::store::DictStore;
            store.tag(name).ok().flatten()
        };
        let v = content(&entries, &[], &dicts, &tags, &|_, _| None);
        let e = &v["entries"][0];
        assert_eq!(e["expression"], "食べる");
        assert_eq!(e["furigana"][0], json!({"t": "食", "r": "た"}));
        assert_eq!(e["trace"].as_array().unwrap().len(), 2);
        assert_eq!(e["termTags"][0]["category"], "popular");
        assert_eq!(e["frequencies"][0]["values"][0], "120");
        assert_eq!(e["pitches"][0]["accents"][0]["highLow"], "LHLL");
        assert_eq!(e["pitches"][0]["accents"][0]["category"], "kifuku");
        let block = &e["blocks"][0];
        assert_eq!(
            (block["dictionary"].as_str(), block["expanded"].as_bool()),
            (Some("JM"), Some(true))
        );
        let first: Vec<&str> = block["glossaries"][0]["tags"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert_eq!(first, ["v1", "vt"], "the sense number is hidden");
        assert!(
            block["glossaries"][1]["tags"]
                .as_array()
                .unwrap()
                .is_empty(),
            "repeated part of speech omitted"
        );
        assert_eq!(v["bestLength"], 5);
    }

    // T-POP-010
    #[test]
    fn images_become_data_urls_from_the_media_table() {
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"M"), "TQ==");
        let png = b"\x89PNG".to_vec();
        let media = |d: usize, path: &str| (d == 0 && path == "img/a.png").then(|| png.clone());
        let mut v = json!([{"k": "flow", "runs": [
            {"k": "image", "image": {"path": "img/a.png", "units": "px"}},
            {"k": "image", "image": {"path": "img/missing.png", "units": "px"}},
            {"k": "image", "image": {"path": "img/a.exe", "units": "px"}}
        ]}]);
        attach_images(&mut v, 0, &media);
        let runs = &v[0]["runs"];
        assert_eq!(runs[0]["image"]["src"], "data:image/png;base64,iVBORw==");
        assert!(
            runs[1]["image"].get("src").is_none(),
            "a missing image keeps its alt text"
        );
        assert!(runs[2]["image"].get("src").is_none(), "only image types");
    }
}
