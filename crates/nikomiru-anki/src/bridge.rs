// SPDX-License-Identifier: GPL-3.0-only

//! AnkiConnect answered inside the app (ANK-055, D-85): on Android there is
//! no AnkiConnect, but AnkiDroid shares its collection through a content
//! provider. [`Dispatcher`] reads the same JSON requests the [`Client`]
//! sends and carries them out on a [`Store`], a collection that can do what
//! AnkiDroid's provider can; the app supplies the store and installs the
//! dispatcher with [`install`]. A client for the address [`ADDRESS`] then
//! talks to it instead of to the network, so mining, the checks and the
//! settings work unchanged.
//!
//! [`Client`]: crate::Client

use std::sync::OnceLock;

use serde_json::{Map, Value, json};

use crate::client::{DUPLICATE, deck_query, field_query};

/// The address of the collection in the app, for the `[anki] url` setting.
pub const ADDRESS: &str = "ankidroid://";

/// What a store answers when the collection's app is not installed, turned
/// into [`AnkiError::AppMissing`](crate::AnkiError::AppMissing).
pub const APP_MISSING: &str = "the collection's app is not installed";

/// What a store answers until the user lets the app use the collection,
/// turned into [`AnkiError::NotAllowed`](crate::AnkiError::NotAllowed).
pub const NOT_ALLOWED: &str = "the collection's app has not allowed access yet";

/// The separator of fields in a stored note (Anki's `\x1f`).
pub const FIELD_SEPARATOR: char = '\u{1f}';

/// A deck.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Deck {
    /// Its id.
    pub id: i64,
    /// Its full name (`Parent::Child`).
    pub name: String,
}

/// A note type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Model {
    /// Its id.
    pub id: i64,
    /// Its name.
    pub name: String,
    /// Its fields, in order.
    pub fields: Vec<String>,
}

/// A stored note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredNote {
    /// Its id.
    pub id: i64,
    /// Its note type's id.
    pub model: i64,
    /// Its fields' values, in the note type's order.
    pub fields: Vec<String>,
    /// Its tags.
    pub tags: Vec<String>,
}

/// A card template of a new note type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    /// Its name.
    pub name: String,
    /// The front.
    pub front: String,
    /// The back.
    pub back: String,
}

/// A collection as AnkiDroid's provider offers it. Errors are messages;
/// [`APP_MISSING`] and [`NOT_ALLOWED`] are recognised.
pub trait Store: Send + Sync {
    /// Every deck.
    fn decks(&self) -> Result<Vec<Deck>, String>;
    /// Adds a deck and returns its id.
    fn add_deck(&self, name: &str) -> Result<i64, String>;
    /// Every note type.
    fn models(&self) -> Result<Vec<Model>, String>;
    /// Adds a note type with its card templates.
    fn add_model(
        &self,
        name: &str,
        fields: &[String],
        css: &str,
        templates: &[Template],
    ) -> Result<i64, String>;
    /// The notes an Anki search finds.
    fn find_notes(&self, query: &str) -> Result<Vec<i64>, String>;
    /// A note, `None` if there is none with that id.
    fn note(&self, id: i64) -> Result<Option<StoredNote>, String>;
    /// Adds a note whose cards go to `deck`; returns its id.
    fn add_note(
        &self,
        model: i64,
        deck: i64,
        fields: &[String],
        tags: &[String],
    ) -> Result<i64, String>;
    /// Replaces a note's fields.
    fn set_fields(&self, id: i64, fields: &[String]) -> Result<(), String>;
    /// Replaces a note's tags.
    fn set_tags(&self, id: i64, tags: &[String]) -> Result<(), String>;
    /// Deletes a note and its cards.
    fn delete_note(&self, id: i64) -> Result<(), String>;
    /// Adds a media file and returns the name it was stored under.
    fn store_media(&self, name: &str, bytes: &[u8]) -> Result<String, String>;
    /// Shows the collection's app, on `query` if it can.
    fn browse(&self, query: &str) -> Result<(), String>;
}

/// Answers AnkiConnect requests on a [`Store`].
#[derive(Debug)]
pub struct Dispatcher<S> {
    store: S,
}

/// What [`install`] keeps: a dispatcher, whatever its store.
pub trait Answer: Send + Sync {
    /// The reply, `{"result": …, "error": …}`, to a request.
    fn answer(&self, request: &Value) -> Value;
}

impl<S: Store> Answer for Dispatcher<S> {
    fn answer(&self, request: &Value) -> Value {
        self.reply(request)
    }
}

static INSTALLED: OnceLock<Box<dyn Answer>> = OnceLock::new();

/// Makes `answer` the collection at [`ADDRESS`] for the rest of the process
/// (the first one stays).
pub fn install(answer: Box<dyn Answer>) {
    if INSTALLED.set(answer).is_err() {
        tracing::warn!(target: "anki", "a collection in the app was installed twice");
    }
}

/// The installed collection, if any.
pub(crate) fn installed() -> Option<&'static dyn Answer> {
    INSTALLED.get().map(Box::as_ref)
}

type Outcome = Result<Value, String>;

impl<S: Store> Dispatcher<S> {
    /// A dispatcher on `store`.
    pub fn new(store: S) -> Self {
        Self { store }
    }

    /// The reply to one request (a `multi` answers each of its actions).
    pub fn reply(&self, request: &Value) -> Value {
        let action = request
            .get("action")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let params = request.get("params").cloned().unwrap_or(Value::Null);
        let outcome = if action == "multi" {
            match params.get("actions") {
                Some(Value::Array(actions)) => Ok(Value::Array(
                    actions.iter().map(|a| self.reply(a)).collect(),
                )),
                _ => Err("multi needs actions".to_string()),
            }
        } else {
            self.run(action, &params)
        };
        match outcome {
            Ok(result) => json!({ "result": result, "error": null }),
            Err(error) => json!({ "result": null, "error": error }),
        }
    }

    fn run(&self, action: &str, params: &Value) -> Outcome {
        match action {
            "version" => Ok(json!(6)),
            "deckNames" => {
                let mut names: Vec<String> =
                    self.store.decks()?.into_iter().map(|d| d.name).collect();
                names.sort();
                Ok(json!(names))
            }
            "createDeck" => {
                let name = text(params, "deck")?;
                match self.store.decks()?.into_iter().find(|d| d.name == name) {
                    Some(deck) => Ok(json!(deck.id)),
                    None => Ok(json!(self.store.add_deck(name)?)),
                }
            }
            "modelNames" => {
                let mut names: Vec<String> =
                    self.store.models()?.into_iter().map(|m| m.name).collect();
                names.sort();
                Ok(json!(names))
            }
            "modelNamesAndIds" => Ok(Value::Object(
                self.store
                    .models()?
                    .into_iter()
                    .map(|m| (m.name, json!(m.id)))
                    .collect(),
            )),
            "modelFieldNames" => Ok(json!(self.model(text(params, "modelName")?)?.fields)),
            "createModel" => self.create_model(params),
            "findNotes" => Ok(json!(self.store.find_notes(text(params, "query")?)?)),
            "canAddNotesWithErrorDetail" => self.can_add(params),
            "addNote" => self.add_note(params),
            "notesInfo" => self.notes_info(params),
            "updateNoteFields" => self.update_fields(params),
            "deleteNotes" => {
                for id in ids(params)? {
                    self.store.delete_note(id)?;
                }
                Ok(Value::Null)
            }
            "addTags" => {
                let tags = split_tags(text(params, "tags")?);
                for id in ids(params)? {
                    let Some(note) = self.store.note(id)? else {
                        continue;
                    };
                    let mut all = note.tags.clone();
                    for tag in &tags {
                        if !all.iter().any(|t| t.eq_ignore_ascii_case(tag)) {
                            all.push(tag.clone());
                        }
                    }
                    if all != note.tags {
                        self.store.set_tags(id, &all)?;
                    }
                }
                Ok(Value::Null)
            }
            "storeMediaFile" => {
                let name = text(params, "filename")?;
                let data = text(params, "data")?;
                let bytes =
                    crate::base64::decode(data).ok_or_else(|| "data is not base64".to_string())?;
                Ok(json!(self.store.store_media(name, &bytes)?))
            }
            // the provider cannot list media: nothing is known to be there,
            // so the media go up again (under a new name)
            "getMediaFilesNames" => Ok(json!([])),
            // nor delete them: Anki's "Check media" removes the unused ones
            "deleteMediaFile" => Ok(Value::Null),
            "guiBrowse" => {
                self.store.browse(text(params, "query")?)?;
                Ok(json!([]))
            }
            _ => Err("unsupported action".to_string()),
        }
    }

    fn model(&self, name: &str) -> Result<Model, String> {
        self.store
            .models()?
            .into_iter()
            .find(|m| m.name == name)
            .ok_or_else(|| format!("model was not found: {name}"))
    }

    fn create_model(&self, params: &Value) -> Outcome {
        let name = text(params, "modelName")?;
        if self.store.models()?.iter().any(|m| m.name == name) {
            return Err(format!("Model name already exists: {name}"));
        }
        let fields = strings(params.get("inOrderFields"));
        let css = params
            .get("css")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let templates: Vec<Template> = params
            .get("cardTemplates")
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .enumerate()
                    .map(|(i, t)| {
                        let get = |key: &str| {
                            t.get(key)
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string()
                        };
                        let name = get("Name");
                        Template {
                            name: if name.is_empty() {
                                format!("Card {}", i + 1)
                            } else {
                                name
                            },
                            front: get("Front"),
                            back: get("Back"),
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        if fields.is_empty() || templates.is_empty() {
            return Err("a note type needs fields and a card template".to_string());
        }
        let id = self.store.add_model(name, &fields, css, &templates)?;
        Ok(json!({ "id": id, "name": name }))
    }

    /// The values of `note`'s fields in its type's order, and the type.
    fn values(&self, note: &Value) -> Result<(Model, Vec<String>), String> {
        let model = self.model(text(note, "modelName")?)?;
        let given = note.get("fields").and_then(Value::as_object);
        let values = model
            .fields
            .iter()
            .map(|f| {
                given
                    .and_then(|g| g.get(f))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string()
            })
            .collect();
        Ok((model, values))
    }

    /// Whether `note` (its first field) has a duplicate, by the options of
    /// `addNote` (ANK-030).
    fn is_duplicate(&self, note: &Value, model: &Model, values: &[String]) -> Result<bool, String> {
        let (Some(field), Some(value)) = (model.fields.first(), values.first()) else {
            return Ok(false);
        };
        let options = note.get("options");
        let in_deck = options
            .and_then(|o| o.get("duplicateScope"))
            .and_then(Value::as_str)
            == Some("deck");
        let deck = options
            .and_then(|o| o.get("duplicateScopeOptions"))
            .and_then(|o| o.get("deckName"))
            .and_then(Value::as_str)
            .or_else(|| note.get("deckName").and_then(Value::as_str))
            .unwrap_or_default();
        let query = if in_deck {
            format!("{} {}", deck_query(deck), field_query(field, value))
        } else {
            field_query(field, value)
        };
        Ok(!self.store.find_notes(&query)?.is_empty())
    }

    fn can_add(&self, params: &Value) -> Outcome {
        let notes = params
            .get("notes")
            .and_then(Value::as_array)
            .ok_or("notes is not a list")?;
        let answers: Result<Vec<Value>, String> = notes
            .iter()
            .map(|note| {
                let (model, values) = match self.values(note) {
                    Ok(v) => v,
                    Err(e) => return Ok(json!({ "canAdd": false, "error": e })),
                };
                if values.first().is_none_or(|v| v.trim().is_empty()) {
                    return Ok(json!({
                        "canAdd": false,
                        "error": "cannot create note because it is empty",
                    }));
                }
                Ok(if self.is_duplicate(note, &model, &values)? {
                    json!({ "canAdd": false, "error": DUPLICATE })
                } else {
                    json!({ "canAdd": true })
                })
            })
            .collect();
        Ok(Value::Array(answers?))
    }

    fn add_note(&self, params: &Value) -> Outcome {
        let note = params.get("note").ok_or("no note")?;
        let (model, values) = self.values(note)?;
        if values.first().is_none_or(|v| v.trim().is_empty()) {
            return Err("cannot create note because it is empty".to_string());
        }
        let allow = note
            .get("options")
            .and_then(|o| o.get("allowDuplicate"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if !allow && self.is_duplicate(note, &model, &values)? {
            return Err(DUPLICATE.to_string());
        }
        let deck_name = text(note, "deckName")?;
        let deck = self
            .store
            .decks()?
            .into_iter()
            .find(|d| d.name == deck_name)
            .ok_or_else(|| format!("deck was not found: {deck_name}"))?;
        let tags = strings(note.get("tags"));
        Ok(json!(
            self.store.add_note(model.id, deck.id, &values, &tags)?
        ))
    }

    fn notes_info(&self, params: &Value) -> Outcome {
        let models = self.store.models()?;
        let mut infos = Vec::new();
        for id in ids(params)? {
            let Some(note) = self.store.note(id)? else {
                infos.push(json!({}));
                continue;
            };
            let Some(model) = models.iter().find(|m| m.id == note.model) else {
                infos.push(json!({}));
                continue;
            };
            let fields: Map<String, Value> = model
                .fields
                .iter()
                .enumerate()
                .map(|(order, name)| {
                    let value = note.fields.get(order).cloned().unwrap_or_default();
                    (name.clone(), json!({ "value": value, "order": order }))
                })
                .collect();
            infos.push(json!({
                "noteId": note.id,
                "modelName": model.name,
                "tags": note.tags,
                "fields": fields,
            }));
        }
        Ok(Value::Array(infos))
    }

    fn update_fields(&self, params: &Value) -> Outcome {
        let note = params.get("note").ok_or("no note")?;
        let id = note
            .get("id")
            .and_then(Value::as_i64)
            .ok_or("the note has no id")?;
        let stored = self
            .store
            .note(id)?
            .ok_or_else(|| format!("Note was not found: {id}"))?;
        let model = self
            .store
            .models()?
            .into_iter()
            .find(|m| m.id == stored.model)
            .ok_or_else(|| format!("model was not found: {}", stored.model))?;
        let given = note.get("fields").and_then(Value::as_object);
        let mut values = stored.fields.clone();
        values.resize(model.fields.len(), String::new());
        for (i, name) in model.fields.iter().enumerate() {
            if let Some(value) = given.and_then(|g| g.get(name)).and_then(Value::as_str) {
                values[i] = value.to_string();
            }
        }
        if values != stored.fields {
            self.store.set_fields(id, &values)?;
        }
        Ok(Value::Null)
    }
}

fn text<'a>(params: &'a Value, key: &str) -> Result<&'a str, String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{key} is missing"))
}

fn strings(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn ids(params: &Value) -> Result<Vec<i64>, String> {
    params
        .get("notes")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_i64).collect())
        .ok_or_else(|| "notes is not a list".to_string())
}

/// Tags as Anki writes them: separated by spaces.
pub fn split_tags(text: &str) -> Vec<String> {
    text.split_whitespace().map(str::to_string).collect()
}

/// A note's fields as the provider stores them.
pub fn join_fields(fields: &[String]) -> String {
    fields.join(&FIELD_SEPARATOR.to_string())
}

/// The fields of a stored note.
pub fn split_fields(text: &str) -> Vec<String> {
    text.split(FIELD_SEPARATOR).map(str::to_string).collect()
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    /// A collection in memory, with AnkiDroid's limits.
    #[derive(Default)]
    struct Memory {
        state: Mutex<State>,
    }

    #[derive(Default)]
    struct State {
        decks: Vec<Deck>,
        models: Vec<Model>,
        notes: Vec<(StoredNote, i64)>,
        media: Vec<(String, usize)>,
        next: i64,
        browsed: Vec<String>,
    }

    impl State {
        fn id(&mut self) -> i64 {
            self.next += 1;
            self.next
        }
    }

    impl Store for Memory {
        fn decks(&self) -> Result<Vec<Deck>, String> {
            Ok(self.state.lock().unwrap().decks.clone())
        }
        fn add_deck(&self, name: &str) -> Result<i64, String> {
            let mut s = self.state.lock().unwrap();
            let id = s.id();
            s.decks.push(Deck {
                id,
                name: name.into(),
            });
            Ok(id)
        }
        fn models(&self) -> Result<Vec<Model>, String> {
            Ok(self.state.lock().unwrap().models.clone())
        }
        fn add_model(
            &self,
            name: &str,
            fields: &[String],
            _css: &str,
            _templates: &[Template],
        ) -> Result<i64, String> {
            let mut s = self.state.lock().unwrap();
            let id = s.id();
            s.models.push(Model {
                id,
                name: name.into(),
                fields: fields.to_vec(),
            });
            Ok(id)
        }
        fn find_notes(&self, query: &str) -> Result<Vec<i64>, String> {
            // enough of Anki's search for the tests: "Field:value" terms
            let s = self.state.lock().unwrap();
            Ok(s.notes
                .iter()
                .filter(|(n, deck)| {
                    let model = s.models.iter().find(|m| m.id == n.model).unwrap();
                    query.split("\" \"").all(|term| {
                        let term = term.trim_matches('"');
                        if let Some(d) = term.strip_prefix("deck:") {
                            return s.decks.iter().any(|x| x.id == *deck && x.name == d);
                        }
                        let Some((field, value)) = term.split_once(':') else {
                            return true;
                        };
                        model
                            .fields
                            .iter()
                            .position(|f| f == field)
                            .is_some_and(|i| n.fields[i] == value)
                    })
                })
                .map(|(n, _)| n.id)
                .collect())
        }
        fn note(&self, id: i64) -> Result<Option<StoredNote>, String> {
            let s = self.state.lock().unwrap();
            Ok(s.notes
                .iter()
                .find(|(n, _)| n.id == id)
                .map(|(n, _)| n.clone()))
        }
        fn add_note(
            &self,
            model: i64,
            deck: i64,
            fields: &[String],
            tags: &[String],
        ) -> Result<i64, String> {
            let mut s = self.state.lock().unwrap();
            let id = s.id();
            s.notes.push((
                StoredNote {
                    id,
                    model,
                    fields: fields.to_vec(),
                    tags: tags.to_vec(),
                },
                deck,
            ));
            Ok(id)
        }
        fn set_fields(&self, id: i64, fields: &[String]) -> Result<(), String> {
            let mut s = self.state.lock().unwrap();
            let note = s.notes.iter_mut().find(|(n, _)| n.id == id).unwrap();
            note.0.fields = fields.to_vec();
            Ok(())
        }
        fn set_tags(&self, id: i64, tags: &[String]) -> Result<(), String> {
            let mut s = self.state.lock().unwrap();
            let note = s.notes.iter_mut().find(|(n, _)| n.id == id).unwrap();
            note.0.tags = tags.to_vec();
            Ok(())
        }
        fn delete_note(&self, id: i64) -> Result<(), String> {
            self.state.lock().unwrap().notes.retain(|(n, _)| n.id != id);
            Ok(())
        }
        fn store_media(&self, name: &str, bytes: &[u8]) -> Result<String, String> {
            let mut s = self.state.lock().unwrap();
            let stored = format!("{}_1", name);
            s.media.push((stored.clone(), bytes.len()));
            Ok(stored)
        }
        fn browse(&self, query: &str) -> Result<(), String> {
            self.state.lock().unwrap().browsed.push(query.into());
            Ok(())
        }
    }

    fn client() -> (crate::Client, std::sync::Arc<Dispatcher<Memory>>) {
        let dispatcher = std::sync::Arc::new(Dispatcher::new(Memory::default()));
        let shared = dispatcher.clone();
        let client = crate::Client::in_app(Box::new(move |request: &Value| shared.reply(request)));
        (client, dispatcher)
    }

    fn note(first: &str) -> crate::Note {
        crate::Note {
            deck: "Mining".into(),
            model: "Basic".into(),
            fields: vec![
                ("Front".into(), first.into()),
                ("Back".into(), "back".into()),
            ],
            tags: vec!["nikomiru".into()],
            scope: crate::DuplicateScope::Deck,
        }
    }

    // T-ANK-055
    #[test]
    fn mining_works_against_a_collection_in_the_app() {
        let (client, dispatcher) = client();
        assert_eq!(client.version().unwrap(), 6);
        client.create_deck("Mining").unwrap();
        client.create_deck("Mining").unwrap();
        assert_eq!(client.deck_names().unwrap(), ["Mining"]);
        client
            .create_model(&crate::NewModel {
                name: "Basic",
                fields: &["Front", "Back"],
                css: "",
                template: "Card 1",
                front: "{{Front}}",
                back: "{{Back}}",
            })
            .unwrap();
        assert_eq!(
            client.model_field_names("Basic").unwrap(),
            ["Front", "Back"]
        );

        let name = client.store_media("a.m4a", b"sound").unwrap();
        assert_eq!(name, "a.m4a_1");
        assert_eq!(client.media_exist(&[name]).unwrap(), [false]);

        assert_eq!(client.duplicates(&[note("猫")]).unwrap(), [false]);
        let id = client.add_note(&note("猫"), false).unwrap();
        assert_eq!(client.duplicates(&[note("猫")]).unwrap(), [true]);
        assert!(matches!(
            client.add_note(&note("猫"), false),
            Err(crate::AnkiError::Duplicate)
        ));
        let again = client.add_note(&note("猫"), true).unwrap();
        assert_ne!(id, again);

        client
            .update_note_fields(id, &[("Back".into(), "ねこ".into())])
            .unwrap();
        client.add_tags(&[id], &["more".into()]).unwrap();
        let info = client.notes_info(&[id, 999]).unwrap();
        let info = info[0].as_ref().unwrap();
        assert_eq!(info.model, "Basic");
        assert_eq!(
            info.fields,
            [
                ("Front".to_string(), "猫".to_string()),
                ("Back".to_string(), "ねこ".to_string())
            ]
        );
        assert_eq!(info.tags, ["nikomiru", "more"]);

        assert_eq!(
            client
                .find_notes(&crate::client::field_query("Front", "猫"))
                .unwrap()
                .len(),
            2
        );
        client.delete_notes(&[again]).unwrap();
        client.delete_media("a.m4a_1").unwrap();
        client.gui_browse("nid:1").unwrap();
        assert_eq!(dispatcher.store.state.lock().unwrap().browsed, ["nid:1"]);
    }

    #[test]
    fn missing_things_become_anki_errors() {
        let (client, _) = client();
        client.create_deck("Mining").unwrap();
        assert!(matches!(
            client.add_note(&note("猫"), false),
            Err(crate::AnkiError::ModelMissing(m)) if m == "Basic"
        ));
        assert!(matches!(
            client.call("frobnicate", None),
            Err(crate::AnkiError::Unsupported(_))
        ));
    }

    #[test]
    fn fields_and_tags_keep_anki_s_form() {
        let fields = vec!["a".to_string(), String::new(), "c d".to_string()];
        assert_eq!(split_fields(&join_fields(&fields)), fields);
        assert_eq!(split_tags("  a b\tc "), ["a", "b", "c"]);
    }
}
