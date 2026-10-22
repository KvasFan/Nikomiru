// SPDX-License-Identifier: GPL-3.0-only

//! L4: "Create Lapis deck" and the refresh of the settings against the fake
//! AnkiConnect (T-ANK-006, T-ANK-011).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use nikomiru_anki::fake::{Behaviour, FakeAnki};
use nikomiru_anki::lapis::{self, Setup};
use nikomiru_anki::{AnkiError, Client};
use serde_json::json;

// T-ANK-011
#[test]
fn create_lapis_deck_makes_what_is_missing_once() {
    let fake = FakeAnki::start().unwrap();
    let c = Client::new(&fake.url(), "").unwrap();
    assert_eq!(lapis::create(&c).map(|m| m.setup), Ok(Setup::Created));
    assert_eq!(
        fake.actions(),
        [
            "modelNames",
            "createModel",
            "deckNames",
            "createDeck",
            "modelNames",
            "deckNames"
        ]
    );
    {
        let anki = fake.collection();
        let model = anki.models.iter().find(|m| m.name == "Lapis").unwrap();
        assert_eq!(model.fields, lapis::FIELDS);
        assert_eq!(model.css, lapis::CSS);
        assert_eq!(
            model.templates,
            [(
                "Mining".to_string(),
                lapis::FRONT.to_string(),
                lapis::BACK.to_string()
            )]
        );
        assert!(anki.decks.iter().any(|d| d == "Lapis"));
    }
    // a second run only reads
    fake.clear_requests();
    assert_eq!(lapis::create(&c).map(|m| m.setup), Ok(Setup::AlreadyExists));
    assert_eq!(
        fake.actions(),
        ["modelNames", "deckNames", "modelNames", "deckNames"]
    );
}

// T-ANK-011
#[test]
fn an_existing_lapis_note_type_gets_only_the_deck() {
    let fake = FakeAnki::start().unwrap();
    let c = Client::new(&fake.url(), "").unwrap();
    c.create_model(&nikomiru_anki::NewModel {
        name: "Lapis",
        fields: &lapis::FIELDS,
        css: "/* the user's */",
        template: "Card 1",
        front: "x",
        back: "y",
    })
    .unwrap();
    fake.clear_requests();
    assert_eq!(lapis::create(&c).map(|m| m.setup), Ok(Setup::Created));
    assert!(!fake.actions().contains(&"createModel".to_string()));
    assert_eq!(fake.collection().models[1].css, "/* the user's */");
}

// T-ANK-011
#[test]
fn a_deck_that_does_not_appear_is_reported() {
    let fake = FakeAnki::start().unwrap();
    let c = Client::new(&fake.url(), "").unwrap();
    fake.then(Behaviour::Json(
        json!({"result": ["Default"], "error": null}),
    )); // modelNames
    fake.then(Behaviour::Json(json!({"result": null, "error": null}))); // createModel
    assert_eq!(lapis::create(&c), Err(AnkiError::LapisMissing));
    // an Anki editor window open (AnkiConnect 26.10)
    fake.then(Behaviour::Json(
        json!({"result": ["Default"], "error": null}),
    ));
    fake.then(Behaviour::Json(json!({"result": null,
        "error": "Close the affected editor before this API write, even if its content is saved."})));
    assert_eq!(lapis::create(&c), Err(AnkiError::EditorOpen));
}

// T-ANK-011: Anki compares deck and note type names without case
#[test]
fn a_lapis_deck_under_another_case_is_the_lapis_deck() {
    let fake = FakeAnki::start().unwrap();
    let c = Client::new(&fake.url(), "").unwrap();
    fake.collection().decks.push("lapis".into());
    let made = lapis::create(&c).unwrap();
    assert_eq!(
        (made.deck.as_str(), made.note_type.as_str()),
        ("lapis", "Lapis")
    );
    assert!(!fake.actions().contains(&"createDeck".to_string()));
}

// T-ANK-006
#[test]
fn the_refresh_reads_decks_note_types_and_the_chosen_fields() {
    let fake = FakeAnki::start().unwrap();
    let c = Client::new(&fake.url(), "").unwrap();
    lapis::create(&c).unwrap();
    fake.clear_requests();
    let state = lapis::refresh(&c, "Lapis").unwrap();
    assert_eq!(state.version, 6);
    assert_eq!(state.decks, ["Default", "Lapis"]);
    assert_eq!(state.models, ["Basic", "Lapis"]);
    assert_eq!(state.fields, lapis::FIELDS);
    assert_eq!(
        fake.actions(),
        ["version", "deckNames", "modelNames", "modelFieldNames"]
    );
    // a note type that is gone has no fields
    assert!(lapis::refresh(&c, "Gone").unwrap().fields.is_empty());
}
