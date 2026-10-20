// SPDX-License-Identifier: GPL-3.0-only

//! L3/L4: the AnkiConnect client against the fake AnkiConnect (TEST §6.9,
//! ADR 0007 §7).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::Duration;

use nikomiru_anki::fake::{Behaviour, FIRST_NOTE_ID, FakeAnki};
use nikomiru_anki::{AnkiError, Client, DuplicateScope, NewModel, Note};
use serde_json::json;

fn client(fake: &FakeAnki) -> Client {
    Client::new(&fake.url(), "").unwrap()
}

fn lapis_note(expression: &str) -> Note {
    Note {
        deck: "Default".into(),
        model: "Basic".into(),
        fields: vec![
            ("Front".into(), expression.into()),
            ("Back".into(), "definition".into()),
        ],
        tags: vec!["nikomiru".into(), "video".into()],
        scope: DuplicateScope::Deck,
    }
}

// T-ANK-001, T-ANK-005
#[test]
fn the_requests_have_anki_connects_shape() {
    let fake = FakeAnki::start().unwrap();
    let c = client(&fake);
    c.version().unwrap();
    c.deck_names().unwrap();
    c.model_names().unwrap();
    c.model_names_and_ids().unwrap();
    c.model_field_names("Basic").unwrap();
    c.duplicates(&[lapis_note("食べ物")]).unwrap();
    let id = c.add_note(&lapis_note("食べ物"), false).unwrap();
    c.find_notes("\"Front:食べ物\"").unwrap();
    c.notes_info(&[id]).unwrap();
    c.update_note_fields(id, &[("Back".into(), "new".into())])
        .unwrap();
    c.add_tags(&[id], &["mine".into(), "again".into()]).unwrap();
    let stored = c
        .store_media("nikomiru_0123456789abcdef.jpg", b"jpg")
        .unwrap();
    c.media_exist(std::slice::from_ref(&stored)).unwrap();
    c.delete_media(&stored).unwrap();
    c.create_deck("Lapis").unwrap();
    c.create_model(&NewModel {
        name: "Mini",
        fields: &["A", "B"],
        css: ".card {}",
        template: "Mining",
        front: "{{A}}",
        back: "{{B}}",
    })
    .unwrap();
    c.gui_browse(&format!("nid:{id}")).unwrap();
    let mut text = String::new();
    for r in fake.requests() {
        text.push_str(&serde_json::to_string(&r).unwrap());
        text.push('\n');
    }
    insta::assert_snapshot!(text);
}

// T-ANK-001
#[test]
fn the_key_goes_in_every_request_and_every_multi_child() {
    let fake = FakeAnki::start().unwrap();
    fake.set_key("s3cret");
    assert_eq!(client(&fake).deck_names(), Err(AnkiError::WrongKey));
    let c = Client::new(&fake.url(), "s3cret").unwrap();
    assert_eq!(c.deck_names().unwrap(), ["Default"]);
    assert_eq!(c.media_exist(&["a.jpg".into()]).unwrap(), [false]);
    let multi = fake.requests().pop().unwrap();
    assert_eq!(multi["key"], "s3cret");
    assert_eq!(multi["params"]["actions"][0]["key"], "s3cret");
    assert_eq!(multi["params"]["actions"][0]["version"], 6);
}

// T-ANK-002
#[test]
fn replies_that_are_not_anki_connects_are_protocol_errors() {
    let fake = FakeAnki::start().unwrap();
    let c = client(&fake);
    let url = fake.url();
    fake.then(Behaviour::Status(403));
    assert_eq!(
        c.deck_names(),
        Err(AnkiError::Http {
            url: url.clone(),
            reason: "HTTP 403".into()
        })
    );
    for (behaviour, reason) in [
        (Behaviour::Body("<html>".into()), "not JSON"),
        (Behaviour::Body(String::new()), "not JSON"),
        (
            Behaviour::Json(json!({"apiVersion": "AnkiConnect v.6"})),
            "no result or error",
        ),
        (
            Behaviour::Json(json!({"result": ["x"]})),
            "no result or error",
        ),
        (Behaviour::Json(json!(["x"])), "no result or error"),
    ] {
        fake.then(behaviour);
        assert_eq!(
            c.deck_names(),
            Err(AnkiError::Protocol {
                url: url.clone(),
                reason: reason.into()
            })
        );
    }
    fake.then(Behaviour::Json(json!({"result": "x", "error": null})));
    assert!(matches!(c.deck_names(), Err(AnkiError::Protocol { .. })));
    fake.then(Behaviour::Json(json!({"result": null, "error": null})));
    assert!(matches!(
        c.add_note(&lapis_note("x"), false),
        Err(AnkiError::Protocol { .. })
    ));
    // the duplicate answer of addNote
    c.add_note(&lapis_note("日"), false).unwrap();
    assert_eq!(
        c.add_note(&lapis_note("日"), false),
        Err(AnkiError::Duplicate)
    );
    // "add anyway"
    assert_eq!(c.add_note(&lapis_note("日"), true), Ok(FIRST_NOTE_ID + 1));
}

// T-ANK-003
#[test]
fn a_read_is_retried_once_when_the_connection_closes_before_the_reply() {
    let fake = FakeAnki::start().unwrap();
    let c = client(&fake);
    fake.then(Behaviour::Close);
    assert_eq!(c.deck_names().unwrap(), ["Default"]);
    assert_eq!(fake.actions(), ["deckNames", "deckNames"]);
    // twice in a row: one retry only
    fake.clear_requests();
    fake.then(Behaviour::Close);
    fake.then(Behaviour::Close);
    assert!(matches!(c.deck_names(), Err(AnkiError::Http { .. })));
    assert_eq!(fake.actions(), ["deckNames", "deckNames"]);
}

// T-ANK-003
#[test]
fn a_reply_timeout_is_not_retried() {
    let fake = FakeAnki::start().unwrap();
    let c = Client::with_timeouts(
        &fake.url(),
        "",
        Duration::from_secs(5),
        Duration::from_millis(300),
    )
    .unwrap();
    fake.then(Behaviour::Hang);
    assert_eq!(c.deck_names(), Err(AnkiError::Timeout { url: fake.url() }));
    assert_eq!(fake.actions(), ["deckNames"]);
    // addNote with no reply: the note may exist
    fake.clear_requests();
    fake.then(Behaviour::Hang);
    assert_eq!(
        c.add_note(&lapis_note("x"), false),
        Err(AnkiError::CommitUnknown {
            action: "addNote".into()
        })
    );
    assert_eq!(fake.actions(), ["addNote"]);
}

// T-ANK-003
#[test]
fn add_note_with_the_reply_lost_is_commit_unknown_after_one_request() {
    let fake = FakeAnki::start().unwrap();
    let c = client(&fake);
    fake.then(Behaviour::ApplyThenClose);
    assert_eq!(
        c.add_note(&lapis_note("食べ物"), false),
        Err(AnkiError::CommitUnknown {
            action: "addNote".into()
        })
    );
    assert_eq!(fake.actions(), ["addNote"], "never sent twice");
    assert_eq!(fake.collection().notes.len(), 1, "Anki did add it");
    // a multi holding addNote is not retried either
    fake.clear_requests();
    fake.then(Behaviour::Close);
    let note = lapis_note("二");
    let r = c.multi(&[(
        "addNote",
        Some(
            json!({"note": {"deckName": note.deck, "modelName": note.model,
            "fields": {"Front": "二"}, "options": {"allowDuplicate": false}}}),
        ),
    )]);
    assert!(matches!(r, Err(AnkiError::CommitUnknown { .. })), "{r:?}");
    assert_eq!(fake.actions(), ["multi"]);
}

// T-ANK-003, T-ANK-004
#[test]
fn nothing_listening_is_connection_refused() {
    let port = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let url = format!("http://127.0.0.1:{port}");
    let c = Client::new(&url, "").unwrap();
    assert_eq!(c.version(), Err(AnkiError::Refused { url: url.clone() }));
    // addNote too: nothing reached Anki, so the outcome is known
    assert_eq!(
        c.add_note(&lapis_note("x"), false),
        Err(AnkiError::Refused { url })
    );
}

// T-ANK-030
#[test]
fn duplicates_use_the_options_of_add_note_and_only_the_duplicate_text() {
    let fake = FakeAnki::start().unwrap();
    let c = client(&fake);
    c.add_note(&lapis_note("食べ物"), false).unwrap();
    fake.clear_requests();
    let mut missing_deck = lapis_note("飲み物");
    missing_deck.deck = "Nowhere".into();
    let found = c
        .duplicates(&[lapis_note("食べ物"), lapis_note("飲み物"), missing_deck])
        .unwrap();
    assert_eq!(
        found,
        [true, false, false],
        "a missing deck is not a duplicate"
    );
    let request = &fake.requests()[0];
    assert_eq!(request["action"], "canAddNotesWithErrorDetail");
    let probe = &request["params"]["notes"][0];
    assert_eq!(
        probe["fields"],
        json!({"Front": "食べ物"}),
        "the first field only"
    );
    assert_eq!(
        probe["options"],
        json!({"allowDuplicate": false, "duplicateScope": "deck",
            "duplicateScopeOptions": {"deckName": "Default", "checkChildren": true, "checkAllModels": true}})
    );
    // the collection scope
    let mut collection = lapis_note("食べ物");
    collection.scope = DuplicateScope::Collection;
    collection.deck = "Lapis".into();
    fake.collection().decks.push("Lapis".into());
    assert_eq!(c.duplicates(&[collection]).unwrap(), [true]);
    assert_eq!(
        fake.requests().last().unwrap()["params"]["notes"][0]["options"]["duplicateScopeOptions"],
        json!({"checkChildren": false, "checkAllModels": true})
    );
}

// T-ANK-030
#[test]
fn an_old_anki_connect_is_asked_with_find_notes() {
    let fake = FakeAnki::start().unwrap();
    fake.unsupported(&["canAddNotesWithErrorDetail"]);
    let c = client(&fake);
    c.add_note(&lapis_note("a\"b"), false).unwrap();
    fake.clear_requests();
    assert_eq!(
        c.duplicates(&[lapis_note("a\"b"), lapis_note("c")])
            .unwrap(),
        [true, false]
    );
    assert_eq!(
        fake.actions(),
        ["canAddNotesWithErrorDetail", "findNotes", "findNotes"]
    );
    assert_eq!(
        fake.requests()[1]["params"]["query"],
        r#""deck:Default" "Front:a\"b""#
    );
}

// T-ANK-036
#[test]
fn media_are_stored_as_base64_and_found_by_exact_name() {
    let fake = FakeAnki::start().unwrap();
    let c = client(&fake);
    let bytes: Vec<u8> = (0..=255).collect();
    assert_eq!(
        c.store_media("nikomiru_a.m4a", &bytes).unwrap(),
        "nikomiru_a.m4a"
    );
    assert_eq!(fake.collection().media["nikomiru_a.m4a"], bytes);
    assert!(fake.requests()[0]["params"].get("path").is_none());
    assert_eq!(
        c.media_exist(&["nikomiru_a.m4a".into(), "nikomiru_a.m4".into()])
            .unwrap(),
        [true, false]
    );
    c.delete_media("nikomiru_a.m4a").unwrap();
    assert!(fake.collection().media.is_empty());
}

// T-ANK-037
#[test]
fn notes_are_read_updated_and_tagged() {
    let fake = FakeAnki::start().unwrap();
    let c = client(&fake);
    let id = c.add_note(&lapis_note("日"), false).unwrap();
    c.update_note_fields(
        id,
        &[("Back".into(), "sun".into()), ("Nope".into(), "x".into())],
    )
    .unwrap();
    c.add_tags(&[id], &["mine".into()]).unwrap();
    c.add_tags(&[id], &[]).unwrap();
    let info = c.notes_info(&[id, 42]).unwrap();
    let note = info[0].as_ref().unwrap();
    assert_eq!(note.model, "Basic");
    assert_eq!(
        note.fields,
        [
            ("Front".to_string(), "日".to_string()),
            ("Back".to_string(), "sun".to_string())
        ]
    );
    assert_eq!(note.tags, ["nikomiru", "video", "mine"]);
    assert_eq!(info[1], None, "a missing note");
    assert_eq!(
        fake.actions(),
        ["addNote", "updateNoteFields", "addTags", "notesInfo"],
        "no addTags without tags"
    );
    assert!(matches!(
        c.update_note_fields(42, &[]),
        Err(AnkiError::Anki(m)) if m == "Note was not found: 42"
    ));
}
