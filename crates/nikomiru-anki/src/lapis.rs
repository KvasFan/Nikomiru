// SPDX-License-Identifier: GPL-3.0-only

//! The Lapis note type (ANK-010…ANK-013): its files, fields and default
//! mapping, how it is recognised, and "Create Lapis deck".
//!
//! Translated from Fushi `packages/fushi_anki/lib/src/lapis_note_type.dart`
//! and `lapis_preset.dart` (commit `ac8034e`); the templates come from the
//! upstream tag instead (`assets/anki/lapis/PROVENANCE.md`).

use std::collections::BTreeMap;

use crate::client::{Client, NewModel};
use crate::error::AnkiError;

/// The note type's name.
pub const NAME: &str = "Lapis";

/// The deck "Create Lapis deck" makes.
pub const DECK: &str = "Lapis";

/// The card template's name, as in the official package.
pub const TEMPLATE: &str = "Mining";

/// The fields, in order (ANK-010).
pub const FIELDS: [&str; 22] = [
    "Expression",
    "ExpressionFurigana",
    "ExpressionReading",
    "ExpressionAudio",
    "SelectionText",
    "MainDefinition",
    "DefinitionPicture",
    "Sentence",
    "SentenceFurigana",
    "SentenceAudio",
    "Picture",
    "Glossary",
    "Hint",
    "IsWordAndSentenceCard",
    "IsClickCard",
    "IsSentenceCard",
    "IsAudioCard",
    "PitchPosition",
    "PitchCategories",
    "Frequency",
    "FreqSort",
    "MiscInfo",
];

/// The front template, as upstream.
pub const FRONT: &str = include_str!("../../../assets/anki/lapis/front.html");

/// The back template, as upstream.
pub const BACK: &str = include_str!("../../../assets/anki/lapis/back.html");

/// The styling, as upstream.
pub const CSS: &str = include_str!("../../../assets/anki/lapis/styling.css");

/// The default mapping (ANK-012). SelectionText, DefinitionPicture,
/// SentenceFurigana, Hint and the other selectors stay empty.
pub const DEFAULT_MAPPING: [(&str, &str); 15] = [
    ("Expression", "{expression}"),
    ("ExpressionFurigana", "{furigana-plain}"),
    ("ExpressionReading", "{reading}"),
    ("ExpressionAudio", "{audio}"),
    ("MainDefinition", "{glossary-first}"),
    ("Sentence", "{sentence}"),
    ("SentenceAudio", "{sentence-audio}"),
    ("Picture", "{card-image}"),
    ("Glossary", "{glossary}"),
    ("PitchPosition", "{pitch-accent-positions}"),
    ("PitchCategories", "{pitch-accent-categories}"),
    ("Frequency", "{frequencies}"),
    ("FreqSort", "{frequency-harmonic-rank}"),
    ("MiscInfo", "{source-link} {clip-timestamp}"),
    ("IsWordAndSentenceCard", "x"),
];

/// Whether a note type is Lapis or works like it (ANK-013): its name contains
/// "lapis", or it has the fields Expression, MainDefinition and Sentence.
pub fn is_lapis(name: &str, fields: &[String]) -> bool {
    name.to_lowercase().contains("lapis")
        || ["Expression", "MainDefinition", "Sentence"]
            .iter()
            .all(|f| fields.iter().any(|g| g == f))
}

/// The mapping of a note type with the Lapis defaults for the fields it has,
/// under the user's own (ANK-012); unchanged if it is not Lapis.
pub fn with_defaults(
    name: &str,
    fields: &[String],
    current: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    if !is_lapis(name, fields) {
        return current.clone();
    }
    let mut mapping: BTreeMap<String, String> = DEFAULT_MAPPING
        .iter()
        .filter(|(field, _)| fields.iter().any(|f| f == field))
        .map(|(field, template)| ((*field).to_string(), (*template).to_string()))
        .collect();
    mapping.extend(current.iter().map(|(k, v)| (k.clone(), v.clone())));
    mapping
}

/// How "Create Lapis deck" ended (ANK-011).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setup {
    /// The note type, the deck or both were made.
    Created,
    /// Both were there already.
    AlreadyExists,
}

/// "Create Lapis deck" (ANK-011): the note type if Anki has none named Lapis,
/// the deck if it is missing, then a check that both exist. Idempotent.
pub fn create(client: &Client) -> Result<Setup, AnkiError> {
    let mut created = false;
    if !client.model_names()?.iter().any(|m| m == NAME) {
        client.create_model(&NewModel {
            name: NAME,
            fields: &FIELDS,
            css: CSS,
            template: TEMPLATE,
            front: FRONT,
            back: BACK,
        })?;
        created = true;
    }
    if !client.deck_names()?.iter().any(|d| d == DECK) {
        client.create_deck(DECK)?;
        created = true;
    }
    let has_model = client.model_names()?.iter().any(|m| m == NAME);
    let has_deck = client.deck_names()?.iter().any(|d| d == DECK);
    if !(has_model && has_deck) {
        return Err(AnkiError::LapisMissing);
    }
    Ok(if created {
        Setup::Created
    } else {
        Setup::AlreadyExists
    })
}

/// What Anki has, for the settings (ANK-006).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AnkiState {
    /// AnkiConnect's API version.
    pub version: u64,
    /// The decks.
    pub decks: Vec<String>,
    /// The note types.
    pub models: Vec<String>,
    /// The fields of the chosen note type, empty if Anki does not have it.
    pub fields: Vec<String>,
}

/// Refreshes what Anki has (ANK-006): `version`, `deckNames`, `modelNames` and
/// the fields of `model`. The settings keep their choices by name.
pub fn refresh(client: &Client, model: &str) -> Result<AnkiState, AnkiError> {
    let version = client.version()?;
    let decks = client.deck_names()?;
    let models = client.model_names()?;
    let fields = if models.iter().any(|m| m == model) {
        client.model_field_names(model)?
    } else {
        Vec::new()
    };
    Ok(AnkiState {
        version,
        decks,
        models,
        fields,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    fn hex(bytes: &[u8]) -> String {
        Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    // T-ANK-010
    #[test]
    fn the_vendored_files_are_the_upstream_ones() {
        assert_eq!(
            hex(FRONT.as_bytes()),
            "7bb9993df961d35e7f49b3edec9f7b0f43987f114f3cfdc627a10219c62aef49"
        );
        assert_eq!(
            hex(BACK.as_bytes()),
            "1d92182a7626b8a14fd80bc82ee07e71a7a387533be3a4124d25fe8b3090a137"
        );
        assert_eq!(
            hex(CSS.as_bytes()),
            "51590e7545d43896cb15e494db49b611a9c6e7bbf05c8e0debbf1f98f7ae8920"
        );
        // every field the templates use is a field of the note type
        for field in FIELDS {
            assert!(
                FRONT.contains(field) || BACK.contains(field),
                "{field} is not in the templates"
            );
        }
        assert_eq!(FIELDS.len(), 22);
        assert_eq!(
            FIELDS[0], "Expression",
            "the first field is the one Anki checks"
        );
    }

    fn lapis_fields() -> Vec<String> {
        FIELDS.iter().map(|f| (*f).to_string()).collect()
    }

    // T-ANK-012: Fushi `fushi/test/anki/lapis_preset_test.dart`
    #[test]
    fn the_defaults_fill_the_lapis_fields() {
        let mapping = with_defaults("Lapis", &lapis_fields(), &BTreeMap::new());
        for (field, template) in DEFAULT_MAPPING {
            assert_eq!(mapping[field], template);
        }
        for empty in [
            "SentenceFurigana",
            "Hint",
            "SelectionText",
            "DefinitionPicture",
            "IsClickCard",
            "IsSentenceCard",
            "IsAudioCard",
        ] {
            assert!(!mapping.contains_key(empty), "{empty}");
        }
        // the user's own mapping wins
        let mine = BTreeMap::from([("Expression".to_string(), "{reading}".to_string())]);
        assert_eq!(
            with_defaults("Lapis", &lapis_fields(), &mine)["Expression"],
            "{reading}"
        );
        // only the fields the note type has
        let fields: Vec<String> = ["Expression", "MainDefinition", "Sentence", "Extra"]
            .map(String::from)
            .to_vec();
        let mapping = with_defaults("My notes", &fields, &BTreeMap::new());
        assert_eq!(
            mapping.keys().collect::<Vec<_>>(),
            ["Expression", "MainDefinition", "Sentence"]
        );
        // another note type keeps its mapping as it is
        let basic: Vec<String> = ["Front", "Back"].map(String::from).to_vec();
        assert_eq!(with_defaults("Basic", &basic, &mine), mine);
    }

    // T-ANK-013
    #[test]
    fn lapis_is_recognised_by_name_or_by_fields() {
        assert!(is_lapis("Lapis", &lapis_fields()));
        assert!(is_lapis("my LAPIS copy", &[]));
        let fields: Vec<String> = ["Expression", "Sentence", "MainDefinition"]
            .map(String::from)
            .to_vec();
        assert!(is_lapis("Mining", &fields));
        let basic: Vec<String> = ["Front", "Back"].map(String::from).to_vec();
        assert!(!is_lapis("Basic", &basic));
        assert!(!is_lapis("Mining", &fields[..2]));
    }
}
