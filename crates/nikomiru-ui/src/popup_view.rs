// SPDX-License-Identifier: GPL-3.0-only

//! The content of a dictionary popup (POP-007, POP-008, POP-010, POP-013):
//! headword with furigana, trace tags, frequency and pitch rows, tags, and one
//! section per dictionary with its glossaries already converted to the safe
//! view model of [`nikomiru_dict::structured`]. Built on the lookup thread,
//! images decoded there too; drawn by `view::popups`. It serializes to JSON
//! for the test harness and the tests only.

use std::collections::HashMap;
use std::io::Cursor;
use std::sync::Arc;

use nikomiru_dict::css::Sheet;
use nikomiru_dict::lookup::{Entry, KanjiEntry};
use nikomiru_dict::structured::{self, Block, Image, Pixels, Run};
use nikomiru_dict::yomitan::{PitchPosition, Tag};
use nikomiru_dict::{frequency, pitch, present};
use serde::Serialize;

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

/// Image files larger than this are not shown.
pub const MAX_IMAGE_BYTES: usize = 4 << 20;
/// Images are decoded only up to this size: a 1 MiB file can expand to
/// gigabytes.
const MAX_DECODED_SIDE: u32 = 8192;
const MAX_DECODER_ALLOC: u64 = 128 << 20;
/// Larger images are scaled down to this side: the popup is at most 560
/// points wide.
const MAX_SIDE: u32 = 1280;
/// Decoded pixels kept for one popup at most.
const MAX_POPUP_PIXELS_BYTES: usize = 64 << 20;

/// A part of the headword with its reading (LKP-050): empty over kana.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Segment {
    /// Text.
    pub t: String,
    /// Reading.
    pub r: String,
}

/// A deinflection step (LKP-024).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Trace {
    /// Name (`-て`).
    pub name: String,
    /// What it means.
    pub description: String,
}

/// A tag, with its category (the chip colour) and notes (the tooltip).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TagChip {
    /// Name.
    pub name: String,
    /// Category (`partOfSpeech`, `popular`…), empty when unknown.
    pub category: String,
    /// Notes.
    pub notes: String,
}

/// The frequencies a dictionary gives (LKP-040).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FrequencyRow {
    /// Dictionary name.
    pub dictionary: String,
    /// The values, as shown.
    pub values: Vec<String>,
}

/// A pitch accent: downstep position and its shape (LKP-041).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Accent {
    /// Downstep position (0: heiban).
    pub position: u32,
    /// `H`/`L` per mora, and one more for the particle after.
    pub high_low: String,
    /// `heiban`, `atamadaka`, `nakadaka`, `odaka` or `kifuku`.
    pub category: String,
}

/// A pitch row: the accents some dictionaries agree on (LKP-042, LKP-043).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PitchRow {
    /// The dictionaries.
    pub dictionaries: Vec<String>,
    /// Accents by downstep.
    pub accents: Vec<Accent>,
    /// Accents written as patterns.
    pub patterns: Vec<String>,
    /// IPA.
    pub transcriptions: Vec<String>,
}

/// A glossary: its definition tags and its content.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Glossary {
    /// Tags shown (POP-008).
    pub tags: Vec<TagChip>,
    /// Content.
    pub blocks: Vec<Block>,
}

/// The glossaries of one dictionary, in a row (POP-008).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Section {
    /// Dictionary name.
    pub dictionary: String,
    /// Open at first (DIC-034).
    pub expanded: bool,
    /// The glossaries.
    pub glossaries: Vec<Glossary>,
}

/// A term entry (POP-007).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryView {
    /// Headword.
    pub expression: String,
    /// Reading.
    pub reading: String,
    /// The text that matched.
    pub matched: String,
    /// The headword with its reading over each part.
    pub furigana: Vec<Segment>,
    /// The morae of the reading (for the pitch notation).
    pub morae: Vec<String>,
    /// Deinflection, innermost first.
    pub trace: Vec<Trace>,
    /// Term tags, once each.
    pub term_tags: Vec<TagChip>,
    /// Frequencies.
    pub frequencies: Vec<FrequencyRow>,
    /// Pitch rows.
    pub pitches: Vec<PitchRow>,
    /// One section per dictionary, in the order of the glossaries.
    pub sections: Vec<Section>,
}

/// A kanji entry (POP-013, LKP-037).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct KanjiView {
    /// Dictionary name.
    pub dictionary: String,
    /// The character.
    pub character: String,
    /// On readings.
    pub onyomi: Vec<String>,
    /// Kun readings.
    pub kunyomi: Vec<String>,
    /// Meanings.
    pub meanings: Vec<String>,
    /// Stroke count.
    pub strokes: Option<String>,
    /// Radical.
    pub radical: Option<String>,
    /// The other statistics: name and value.
    pub stats: Vec<(String, String)>,
    /// Frequencies.
    pub frequencies: Vec<FrequencyRow>,
}

/// What a popup shows.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Content {
    /// Term entries.
    pub entries: Vec<EntryView>,
    /// Kanji entries.
    pub kanji: Vec<KanjiView>,
    /// Characters of the query that matched (POP-002).
    pub best_length: usize,
}

/// Decodes a dictionary image, by its bytes (not its name): PNG, JPEG, GIF
/// (the first frame), WebP or BMP, within limits; scaled down when large.
pub fn decode_image(bytes: &[u8]) -> Option<Pixels> {
    use image::ImageFormat;
    if bytes.len() > MAX_IMAGE_BYTES {
        return None;
    }
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    if !matches!(
        reader.format(),
        Some(
            ImageFormat::Png
                | ImageFormat::Jpeg
                | ImageFormat::Gif
                | ImageFormat::WebP
                | ImageFormat::Bmp
        )
    ) {
        return None;
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_DECODED_SIDE);
    limits.max_image_height = Some(MAX_DECODED_SIDE);
    limits.max_alloc = Some(MAX_DECODER_ALLOC);
    reader.limits(limits);
    let mut decoded = reader.decode().ok()?;
    if decoded.width() > MAX_SIDE || decoded.height() > MAX_SIDE {
        decoded = decoded.thumbnail(MAX_SIDE, MAX_SIDE);
    }
    let rgba = decoded.into_rgba8();
    Some(Pixels {
        size: [rgba.width() as usize, rgba.height() as usize],
        rgba: rgba.into_raw(),
    })
}

/// Gives the images of a popup their pixels (POP-010), from the dictionaries'
/// media tables: the popup never loads an address from a dictionary. Missing,
/// unknown or too large images keep their replacement text.
struct Images<'a> {
    media: MediaLookup<'a>,
    decoded: HashMap<(usize, String), Option<Arc<Pixels>>>,
    bytes: usize,
}

impl Images<'_> {
    fn attach(&mut self, blocks: &mut [Block], dictionary: usize) {
        for b in blocks {
            match b {
                Block::Flow { runs } => self.attach_runs(runs, dictionary),
                Block::Box { blocks, .. } => self.attach(blocks, dictionary),
                Block::List { items, .. } => {
                    for item in items {
                        self.attach(item, dictionary);
                    }
                }
                Block::Table { rows, .. } => {
                    for cell in rows.iter_mut().flatten() {
                        self.attach(&mut cell.blocks, dictionary);
                    }
                }
                Block::Details {
                    summary, blocks, ..
                } => {
                    self.attach_runs(summary, dictionary);
                    self.attach(blocks, dictionary);
                }
                Block::Pairs { .. } => {}
            }
        }
    }

    fn attach_runs(&mut self, runs: &mut [Run], dictionary: usize) {
        for r in runs {
            if let Run::Image { image } = r {
                image.pixels = self.pixels(dictionary, image);
            }
        }
    }

    fn pixels(&mut self, dictionary: usize, image: &Image) -> Option<Arc<Pixels>> {
        let key = (dictionary, image.path.clone());
        if let Some(known) = self.decoded.get(&key) {
            return known.clone();
        }
        let pixels = (self.bytes < MAX_POPUP_PIXELS_BYTES)
            .then(|| (self.media)(dictionary, &image.path))
            .flatten()
            .and_then(|bytes| decode_image(&bytes))
            .filter(|p| self.bytes + p.rgba.len() <= MAX_POPUP_PIXELS_BYTES)
            .map(Arc::new);
        if let Some(p) = &pixels {
            self.bytes += p.rgba.len();
        }
        self.decoded.insert(key, pixels.clone());
        pixels
    }
}

fn tag_chip(dictionary: usize, name: &str, tags: TagLookup<'_>) -> TagChip {
    let info = tags(dictionary, name);
    TagChip {
        name: name.to_string(),
        category: info
            .as_ref()
            .map(|t| t.category.clone())
            .unwrap_or_default(),
        notes: info.map(|t| t.notes).unwrap_or_default(),
    }
}

/// Definition tags shown (POP-008): numbers hidden, and a part-of-speech tag
/// already shown on the previous sense of the same dictionary omitted.
fn definition_chips(
    dictionary: usize,
    names: &str,
    previous_pos: &mut Vec<String>,
    tags: TagLookup<'_>,
) -> Vec<TagChip> {
    let mut chips = Vec::new();
    let mut pos_now = Vec::new();
    for name in names.split_whitespace() {
        if name.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let chip = tag_chip(dictionary, name, tags);
        if chip.category == "partOfSpeech" {
            pos_now.push(name.to_string());
            if previous_pos.iter().any(|p| p == name) {
                continue;
            }
        }
        chips.push(chip);
    }
    *previous_pos = pos_now;
    chips
}

fn frequency_rows(
    frequencies: &[nikomiru_dict::lookup::Frequencies],
    name: &dyn Fn(usize) -> String,
) -> Vec<FrequencyRow> {
    frequencies
        .iter()
        .map(|f| FrequencyRow {
            dictionary: name(f.dictionary),
            values: f.values.iter().map(frequency::shown).collect(),
        })
        .collect()
}

fn entry_view(
    entry: &Entry,
    dicts: &[DictView],
    tags: TagLookup<'_>,
    images: &mut Images<'_>,
) -> EntryView {
    let name = |i: usize| dicts.get(i).map(|d| d.name.clone()).unwrap_or_default();
    let furigana = nikomiru_text::furigana::distribute(&entry.expression, &entry.reading)
        .into_iter()
        .map(|s| Segment {
            t: s.text,
            r: s.reading,
        })
        .collect();
    let trace = present::trace_tags(entry)
        .iter()
        .map(|s| Trace {
            name: s.name.to_string(),
            description: s.description.to_string(),
        })
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
    let pitches = pitch::rows(&groups, true)
        .into_iter()
        .map(|r| PitchRow {
            accents: r
                .positions
                .iter()
                .map(|&p| Accent {
                    position: p,
                    high_low: pitch::high_low(morae.len(), p),
                    category: pitch::category(morae.len(), p, &entry.rules)
                        .name()
                        .to_string(),
                })
                .collect(),
            dictionaries: r.dictionaries,
            patterns: r.patterns,
            transcriptions: r.transcriptions,
        })
        .collect();

    // one section per dictionary, in the order of the glossaries
    let mut sections: Vec<(usize, Vec<Glossary>, Vec<String>)> = Vec::new();
    let mut term_tags: Vec<TagChip> = Vec::new();
    for g in &entry.glossaries {
        let Ok(text) = g.term.glossary() else {
            continue;
        };
        let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        if present::is_redirect_label(&g.term.definition_tags, &g.term.term_tags, &json) {
            continue;
        }
        for t in g.term.term_tags.split_whitespace() {
            if !term_tags.iter().any(|s| s.name == t) {
                term_tags.push(tag_chip(g.dictionary, t, tags));
            }
        }
        let sheet = dicts.get(g.dictionary).map(|d| &d.sheet);
        if sections.last().is_none_or(|(d, _, _)| *d != g.dictionary) {
            sections.push((g.dictionary, Vec::new(), Vec::new()));
        }
        if let Some((d, glossaries, previous_pos)) = sections.last_mut() {
            let chips = definition_chips(*d, &g.term.definition_tags, previous_pos, tags);
            let mut blocks = structured::render(&json, sheet.unwrap_or(&Sheet::default()));
            images.attach(&mut blocks, *d);
            glossaries.push(Glossary {
                tags: chips,
                blocks,
            });
        }
    }
    let sections = sections
        .into_iter()
        .enumerate()
        .map(|(n, (d, glossaries, _))| {
            let view = dicts.get(d).cloned().unwrap_or_default();
            let expanded = match view.collapse {
                Collapse::Expanded => true,
                Collapse::Collapsed => false,
                Collapse::Automatic => n < AUTOMATIC_EXPANDED,
            };
            Section {
                dictionary: view.name,
                expanded,
                glossaries,
            }
        })
        .collect();
    EntryView {
        expression: entry.expression.clone(),
        reading: entry.reading.clone(),
        matched: entry.matched.clone(),
        furigana,
        morae,
        trace,
        term_tags,
        frequencies: frequency_rows(&entry.frequencies, &name),
        pitches,
        sections,
    }
}

fn kanji_view(k: &KanjiEntry, dicts: &[DictView]) -> KanjiView {
    let name = |i: usize| dicts.get(i).map(|d| d.name.clone()).unwrap_or_default();
    let stat = |n: &str| {
        k.kanji
            .stats
            .iter()
            .find(|(s, _)| s == n)
            .map(|(_, v)| v.clone())
    };
    KanjiView {
        dictionary: name(k.dictionary),
        character: k.kanji.character.clone(),
        onyomi: k
            .kanji
            .onyomi
            .split_whitespace()
            .map(String::from)
            .collect(),
        kunyomi: k
            .kanji
            .kunyomi
            .split_whitespace()
            .map(String::from)
            .collect(),
        meanings: k.kanji.meanings.clone(),
        strokes: stat("strokes"),
        radical: stat("radical"),
        stats: k
            .kanji
            .stats
            .iter()
            .filter(|(n, _)| n != "strokes" && n != "radical")
            .cloned()
            .collect(),
        frequencies: frequency_rows(&k.frequencies, &name),
    }
}

/// The popup content for these entries and kanji entries.
pub fn content(
    entries: &[Entry],
    kanji: &[KanjiEntry],
    dicts: &[DictView],
    tags: TagLookup<'_>,
    media: MediaLookup<'_>,
) -> Content {
    let mut images = Images {
        media,
        decoded: HashMap::new(),
        bytes: 0,
    };
    Content {
        entries: entries
            .iter()
            .map(|e| entry_view(e, dicts, tags, &mut images))
            .collect(),
        kanji: kanji.iter().map(|k| kanji_view(k, dicts)).collect(),
        best_length: nikomiru_dict::lookup::best_length(entries),
    }
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
        let e = &v.entries[0];
        assert_eq!(e.expression, "食べる");
        assert_eq!(
            e.furigana[0],
            Segment {
                t: "食".into(),
                r: "た".into()
            }
        );
        assert_eq!(e.trace.len(), 2);
        assert_eq!(e.term_tags[0].category, "popular");
        assert_eq!(e.frequencies[0].values, ["120"]);
        assert_eq!(e.pitches[0].accents[0].high_low, "LHLL");
        assert_eq!(e.pitches[0].accents[0].category, "kifuku");
        let section = &e.sections[0];
        assert_eq!(
            (section.dictionary.as_str(), section.expanded),
            ("JM", true)
        );
        let first: Vec<&str> = section.glossaries[0]
            .tags
            .iter()
            .map(|t| t.name.as_str())
            .collect();
        assert_eq!(first, ["v1", "vt"], "the sense number is hidden");
        assert!(
            section.glossaries[1].tags.is_empty(),
            "repeated part of speech omitted"
        );
        assert_eq!(v.best_length, 5);
    }

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        image::RgbaImage::from_pixel(width, height, image::Rgba([255, 0, 0, 255]))
            .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
            .unwrap();
        bytes
    }

    // T-POP-010
    #[test]
    fn images_are_decoded_from_the_media_table() {
        let small = png(2, 3);
        let decoded = decode_image(&small).unwrap();
        assert_eq!(decoded.size, [2, 3]);
        assert_eq!(&decoded.rgba[..4], &[255, 0, 0, 255]);
        // by content, not by name; never anything but the allowed formats
        assert!(decode_image(b"<svg xmlns='http://www.w3.org/2000/svg'/>").is_none());
        assert!(decode_image(b"\x89PNG garbage").is_none());
        // large images are scaled down
        let big = decode_image(&png(4000, 100)).unwrap();
        assert_eq!(big.size, [1280, 32]);

        let image = |path: &str| Run::Image {
            image: Image {
                path: path.into(),
                width: None,
                height: None,
                units: nikomiru_dict::structured::Units::Px,
                pixelated: false,
                collapsed: false,
                title: String::new(),
                alt: "alt".into(),
                pixels: None,
            },
        };
        let mut blocks = vec![Block::Flow {
            runs: vec![
                image("img/a.png"),
                image("img/missing.png"),
                image("img/a.png"),
            ],
        }];
        let media = |d: usize, path: &str| (d == 0 && path == "img/a.png").then(|| small.clone());
        let mut images = Images {
            media: &media,
            decoded: HashMap::new(),
            bytes: 0,
        };
        images.attach(&mut blocks, 0);
        let Block::Flow { runs } = &blocks[0] else {
            panic!()
        };
        let pixels: Vec<Option<&Arc<Pixels>>> = runs
            .iter()
            .map(|r| match r {
                Run::Image { image } => image.pixels.as_ref(),
                _ => None,
            })
            .collect();
        assert!(pixels[0].is_some());
        assert!(pixels[1].is_none(), "a missing image keeps its alt text");
        assert!(
            Arc::ptr_eq(pixels[0].unwrap(), pixels[2].unwrap()),
            "decoded once"
        );
        assert_eq!(images.bytes, 2 * 3 * 4);
    }
}
