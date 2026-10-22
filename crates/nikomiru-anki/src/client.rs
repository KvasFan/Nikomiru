// SPDX-License-Identifier: GPL-3.0-only

//! The AnkiConnect client (ANK-001…ANK-006): the legacy API, version 6, one
//! request at a time over a blocking HTTP client without proxy or pooled
//! connections, retried by the phase that failed ([ADR 0007]).
//!
//! Translated from Fushi `packages/fushi_anki/lib/src/ankiconnect/ankiconnect_service.dart`
//! (commit `ac8034e`): the request shape, the error mapping and the duplicate
//! options. The retry rules differ (no pooled connections, ADR 0007 §6).
//!
//! [ADR 0007]: ../../../docs/adr/0007-mining-to-anki.md

use std::net::IpAddr;
use std::time::Duration;

use serde_json::{Map, Value, json};

use crate::error::AnkiError;

/// AnkiConnect's answer to `addNote` for a duplicate (ANK-002).
pub(crate) const DUPLICATE: &str = "cannot create note because it is a duplicate";

/// The API version sent with every request.
const VERSION: u32 = 6;

/// Time to connect (ANK-001).
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Time for a whole request (ANK-001).
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Actions that change Anki and must not be sent twice (ANK-003).
const NOT_IDEMPOTENT: [&str; 2] = ["addNote", "createModel"];

/// Where duplicates are looked for (`[anki] duplicate_scope`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DuplicateScope {
    /// In the deck and its children.
    #[default]
    Deck,
    /// In the whole collection.
    Collection,
}

/// A note to add, or to check for duplicates.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Note {
    /// The deck.
    pub deck: String,
    /// The note type.
    pub model: String,
    /// Field names and values, the first field first.
    pub fields: Vec<(String, String)>,
    /// Tags.
    pub tags: Vec<String>,
    /// Where duplicates are looked for.
    pub scope: DuplicateScope,
}

impl Note {
    /// The `options` of `addNote` and `canAddNotesWithErrorDetail`, the same for
    /// both (ANK-030).
    fn options(&self, allow_duplicate: bool) -> Value {
        match self.scope {
            DuplicateScope::Deck => json!({
                "allowDuplicate": allow_duplicate,
                "duplicateScope": "deck",
                "duplicateScopeOptions": {
                    "deckName": self.deck,
                    "checkChildren": true,
                    "checkAllModels": true,
                },
            }),
            DuplicateScope::Collection => json!({
                "allowDuplicate": allow_duplicate,
                "duplicateScope": "collection",
                "duplicateScopeOptions": {
                    "checkChildren": false,
                    "checkAllModels": true,
                },
            }),
        }
    }

    fn fields_json(&self) -> Value {
        Value::Object(
            self.fields
                .iter()
                .map(|(k, v)| (k.clone(), Value::String(v.clone())))
                .collect(),
        )
    }
}

/// A note as `notesInfo` returns it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteInfo {
    /// Its id.
    pub id: u64,
    /// Its note type.
    pub model: String,
    /// Field names and values, in the note type's order.
    pub fields: Vec<(String, String)>,
    /// Its tags.
    pub tags: Vec<String>,
}

/// The Anki search for `value` in `field`, exactly (`"Field:value"`, with
/// Anki's special characters escaped).
pub fn field_query(field: &str, value: &str) -> String {
    let escape = |s: &str| {
        let mut out = String::with_capacity(s.len());
        for c in s.chars() {
            if matches!(c, '\\' | '"' | '*' | '_') {
                out.push('\\');
            }
            out.push(c);
        }
        out
    };
    format!("\"{}:{}\"", escape(field), escape(value))
}

/// `deck:"name"`, for a search within a deck and its children.
pub fn deck_query(deck: &str) -> String {
    format!(
        "\"deck:{}\"",
        deck.replace('\\', "\\\\").replace('"', "\\\"")
    )
}

/// How a request failed.
enum Failure {
    /// The connection was not made: nothing reached Anki.
    Connect,
    /// The connection closed before the whole reply.
    Lost(String),
    /// No reply in time.
    Timeout,
}

/// A client for one AnkiConnect.
pub struct Client {
    http: reqwest::blocking::Client,
    url: reqwest::Url,
    shown: String,
    key: String,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // never the key
        f.debug_struct("Client").field("url", &self.shown).finish()
    }
}

impl Client {
    /// A client for the AnkiConnect at `url` with the API key `key` (empty: none).
    pub fn new(url: &str, key: &str) -> Result<Self, AnkiError> {
        Self::with_timeouts(url, key, CONNECT_TIMEOUT, REQUEST_TIMEOUT)
    }

    /// [`Client::new`] with other timeouts, for the tests.
    pub fn with_timeouts(
        url: &str,
        key: &str,
        connect: Duration,
        request: Duration,
    ) -> Result<Self, AnkiError> {
        let bad = || AnkiError::BadUrl(nikomiru_core::config::without_userinfo(url.trim()));
        let parsed = reqwest::Url::parse(url.trim()).map_err(|_| bad())?;
        let host = match parsed.host_str() {
            Some(h) if matches!(parsed.scheme(), "http" | "https") => h.to_string(),
            _ => return Err(bad()),
        };
        // the address shown in messages: no user name or password
        let shown = match parsed.port_or_known_default() {
            Some(port) => format!("{}://{host}:{port}", parsed.scheme()),
            None => format!("{}://{host}", parsed.scheme()),
        };
        let http = reqwest::blocking::Client::builder()
            .no_proxy()
            .pool_max_idle_per_host(0)
            .connect_timeout(connect)
            .timeout(request)
            .build()
            .map_err(|e| AnkiError::Http {
                url: shown.clone(),
                reason: e.to_string(),
            })?;
        Ok(Self {
            http,
            url: parsed,
            shown,
            key: key.to_string(),
        })
    }

    /// The address, for messages.
    pub fn url(&self) -> &str {
        &self.shown
    }

    /// Whether AnkiConnect is on this computer (`localhost`, `127.0.0.0/8`,
    /// `::1`): only then is it asked anything without an action of the user
    /// (PRV-001).
    pub fn is_loopback(&self) -> bool {
        let host = self.url.host_str().unwrap_or_default();
        let host = host.trim_start_matches('[').trim_end_matches(']');
        host.eq_ignore_ascii_case("localhost")
            || host.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback())
    }

    /// The JSON body of a request: `params` only if given, `key` only if set.
    fn body(&self, action: &str, params: Option<&Value>) -> Value {
        let mut body = Map::new();
        body.insert("action".into(), Value::String(action.to_string()));
        body.insert("version".into(), json!(VERSION));
        if let Some(params) = params {
            body.insert("params".into(), params.clone());
        }
        if !self.key.is_empty() {
            body.insert("key".into(), Value::String(self.key.clone()));
        }
        Value::Object(body)
    }

    /// Sends `action` with `params` and returns its `result` (ANK-002, ANK-003).
    pub fn call(&self, action: &str, params: Option<Value>) -> Result<Value, AnkiError> {
        let body = self.body(action, params.as_ref());
        let idempotent = !NOT_IDEMPOTENT.contains(&action);
        let raw = self.send(action, &body, idempotent)?;
        let (error, result) = self
            .envelope(raw)
            .map_err(|e| unread(action, idempotent, e))?;
        answer(action, error, result)
    }

    /// Sends several actions in one request (`multi`); each gets its own result.
    pub fn multi(
        &self,
        actions: &[(&str, Option<Value>)],
    ) -> Result<Vec<Result<Value, AnkiError>>, AnkiError> {
        let children: Vec<Value> = actions
            .iter()
            .map(|(action, params)| self.body(action, params.as_ref()))
            .collect();
        let idempotent = actions.iter().all(|(a, _)| !NOT_IDEMPOTENT.contains(a));
        let params = json!({ "actions": children });
        let body = self.body("multi", Some(&params));
        let raw = self.send("multi", &body, idempotent)?;
        let (error, result) = self
            .envelope(raw)
            .map_err(|e| unread("multi", idempotent, e))?;
        let Value::Array(replies) = answer("multi", error, result)? else {
            let e = self.protocol("multi did not return a list");
            return Err(unread("multi", idempotent, e));
        };
        if replies.len() != actions.len() {
            let e = self.protocol("multi returned a different number of results");
            return Err(unread("multi", idempotent, e));
        }
        Ok(replies
            .into_iter()
            .zip(actions)
            .map(|(reply, (action, _))| match reply {
                Value::Object(mut o) if o.contains_key("result") && o.contains_key("error") => {
                    let error = o.remove("error").unwrap_or(Value::Null);
                    let result = o.remove("result").unwrap_or(Value::Null);
                    answer(action, error, result)
                }
                _ => Err(unread(
                    action,
                    !NOT_IDEMPOTENT.contains(action),
                    self.protocol("a result of multi has no result or error"),
                )),
            })
            .collect())
    }

    fn protocol(&self, reason: &str) -> AnkiError {
        AnkiError::Protocol {
            url: self.shown.clone(),
            reason: reason.to_string(),
        }
    }

    /// `error` and `result` of a reply, or a protocol error.
    fn envelope(&self, raw: String) -> Result<(Value, Value), AnkiError> {
        let value: Value = serde_json::from_str(&raw).map_err(|_| self.protocol("not JSON"))?;
        match value {
            Value::Object(mut o) if o.contains_key("result") && o.contains_key("error") => Ok((
                o.remove("error").unwrap_or(Value::Null),
                o.remove("result").unwrap_or(Value::Null),
            )),
            _ => Err(self.protocol("no result or error")),
        }
    }

    /// Sends a body and returns the reply's text, retrying once by the phase
    /// that failed (ANK-003, ADR 0007 §6).
    fn send(&self, action: &str, body: &Value, idempotent: bool) -> Result<String, AnkiError> {
        let bytes = serde_json::to_vec(body).unwrap_or_default();
        let mut retried = false;
        loop {
            let failure = match self.attempt(&bytes) {
                Ok((200, text)) => return Ok(text),
                // a proxy in front may have passed the request on (ANK-003)
                Ok((500.., _)) if !idempotent => {
                    return Err(AnkiError::CommitUnknown {
                        action: action.to_string(),
                    });
                }
                Ok((status, _)) => {
                    return Err(AnkiError::Http {
                        url: self.shown.clone(),
                        reason: format!("HTTP {status}"),
                    });
                }
                Err(f) => f,
            };
            let again = match &failure {
                Failure::Connect => true,
                Failure::Lost(_) => idempotent,
                Failure::Timeout => false,
            };
            if again && !retried {
                tracing::debug!(target: "anki", "{action}: one more attempt");
                retried = true;
                continue;
            }
            return Err(match failure {
                Failure::Connect => AnkiError::Refused {
                    url: self.shown.clone(),
                },
                _ if !idempotent => AnkiError::CommitUnknown {
                    action: action.to_string(),
                },
                Failure::Lost(reason) => AnkiError::Http {
                    url: self.shown.clone(),
                    reason,
                },
                Failure::Timeout => AnkiError::Timeout {
                    url: self.shown.clone(),
                },
            });
        }
    }

    fn attempt(&self, body: &[u8]) -> Result<(u16, String), Failure> {
        let classify = |e: reqwest::Error| {
            if e.is_connect() {
                Failure::Connect
            } else if e.is_timeout() {
                Failure::Timeout
            } else {
                Failure::Lost(without_url(&e))
            }
        };
        let response = self
            .http
            .post(self.url.clone())
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .header(reqwest::header::CONNECTION, "close")
            .body(body.to_vec())
            .send()
            .map_err(classify)?;
        let status = response.status().as_u16();
        let text = response.text().map_err(classify)?;
        Ok((status, text))
    }

    /// `version`: AnkiConnect's API version (6 for every release since 2018).
    pub fn version(&self) -> Result<u64, AnkiError> {
        let v = self.call("version", None)?;
        v.as_u64()
            .ok_or_else(|| self.protocol("version is not a number"))
    }

    fn strings(&self, value: Value, what: &str) -> Result<Vec<String>, AnkiError> {
        match value {
            Value::Array(items) => Ok(items
                .into_iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()),
            _ => Err(self.protocol(&format!("{what} is not a list"))),
        }
    }

    fn ids(&self, value: Value, what: &str) -> Result<Vec<u64>, AnkiError> {
        match value {
            Value::Array(items) => Ok(items.iter().filter_map(Value::as_u64).collect()),
            _ => Err(self.protocol(&format!("{what} is not a list"))),
        }
    }

    /// `deckNames`.
    pub fn deck_names(&self) -> Result<Vec<String>, AnkiError> {
        let v = self.call("deckNames", None)?;
        self.strings(v, "deckNames")
    }

    /// `modelNames`.
    pub fn model_names(&self) -> Result<Vec<String>, AnkiError> {
        let v = self.call("modelNames", None)?;
        self.strings(v, "modelNames")
    }

    /// `modelNamesAndIds`, sorted by name.
    pub fn model_names_and_ids(&self) -> Result<Vec<(String, u64)>, AnkiError> {
        match self.call("modelNamesAndIds", None)? {
            Value::Object(map) => Ok(map
                .into_iter()
                .filter_map(|(name, id)| id.as_u64().map(|id| (name, id)))
                .collect()),
            _ => Err(self.protocol("modelNamesAndIds is not an object")),
        }
    }

    /// `modelFieldNames` of a note type.
    pub fn model_field_names(&self, model: &str) -> Result<Vec<String>, AnkiError> {
        let v = self.call("modelFieldNames", Some(json!({ "modelName": model })))?;
        self.strings(v, "modelFieldNames")
    }

    /// For each note, whether Anki already has a duplicate of it (ANK-030):
    /// `canAddNotesWithErrorDetail` on the first field, with the options of
    /// `addNote`; only the duplicate text counts. An AnkiConnect without that
    /// action (older than 24.2.26.0) is asked with `findNotes` instead.
    pub fn duplicates(&self, notes: &[Note]) -> Result<Vec<bool>, AnkiError> {
        let probes: Vec<Value> = notes
            .iter()
            .map(|n| {
                let first: Vec<(String, String)> = n.fields.iter().take(1).cloned().collect();
                json!({
                    "deckName": n.deck,
                    "modelName": n.model,
                    "fields": Note { fields: first, ..n.clone() }.fields_json(),
                    "options": n.options(false),
                })
            })
            .collect();
        match self.call(
            "canAddNotesWithErrorDetail",
            Some(json!({ "notes": probes })),
        ) {
            Ok(Value::Array(answers)) if answers.len() == notes.len() => Ok(answers
                .iter()
                .map(|a| {
                    a.get("canAdd") == Some(&Value::Bool(false))
                        && a.get("error").and_then(Value::as_str) == Some(DUPLICATE)
                })
                .collect()),
            Ok(_) => Err(self.protocol("canAddNotesWithErrorDetail: not one answer per note")),
            Err(AnkiError::Unsupported(_)) => notes
                .iter()
                .map(|n| {
                    let Some((field, value)) = n.fields.first() else {
                        return Ok(false);
                    };
                    let query = match n.scope {
                        DuplicateScope::Deck => {
                            format!("{} {}", deck_query(&n.deck), field_query(field, value))
                        }
                        DuplicateScope::Collection => field_query(field, value),
                    };
                    Ok(!self.find_notes(&query)?.is_empty())
                })
                .collect(),
            Err(e) => Err(e),
        }
    }

    /// `findNotes`.
    pub fn find_notes(&self, query: &str) -> Result<Vec<u64>, AnkiError> {
        let v = self.call("findNotes", Some(json!({ "query": query })))?;
        self.ids(v, "findNotes")
    }

    /// `addNote`; `allow_duplicate` for "add anyway" (ANK-030).
    pub fn add_note(&self, note: &Note, allow_duplicate: bool) -> Result<u64, AnkiError> {
        let params = json!({
            "note": {
                "deckName": note.deck,
                "modelName": note.model,
                "fields": note.fields_json(),
                "tags": note.tags,
                "options": note.options(allow_duplicate),
            }
        });
        let id = self.call("addNote", Some(params))?;
        // a reply without the id: whether the note was added is unknown
        id.as_u64().ok_or_else(|| AnkiError::CommitUnknown {
            action: "addNote".into(),
        })
    }

    /// `notesInfo`: `None` for a note that does not exist.
    pub fn notes_info(&self, ids: &[u64]) -> Result<Vec<Option<NoteInfo>>, AnkiError> {
        let Value::Array(notes) = self.call("notesInfo", Some(json!({ "notes": ids })))? else {
            return Err(self.protocol("notesInfo is not a list"));
        };
        Ok(notes.iter().map(note_info).collect())
    }

    /// `updateNoteFields` (ANK-037).
    pub fn update_note_fields(
        &self,
        id: u64,
        fields: &[(String, String)],
    ) -> Result<(), AnkiError> {
        let fields: Map<String, Value> = fields
            .iter()
            .map(|(k, v)| (k.clone(), Value::String(v.clone())))
            .collect();
        self.call(
            "updateNoteFields",
            Some(json!({ "note": { "id": id, "fields": fields } })),
        )
        .map(drop)
    }

    /// `addTags` (nothing to send for no tags).
    pub fn add_tags(&self, ids: &[u64], tags: &[String]) -> Result<(), AnkiError> {
        if tags.is_empty() {
            return Ok(());
        }
        self.call(
            "addTags",
            Some(json!({ "notes": ids, "tags": tags.join(" ") })),
        )
        .map(drop)
    }

    /// `storeMediaFile` with the bytes as base64 (ANK-036); returns the name
    /// Anki stored it under.
    pub fn store_media(&self, name: &str, bytes: &[u8]) -> Result<String, AnkiError> {
        let stored = self.call(
            "storeMediaFile",
            Some(json!({ "filename": name, "data": crate::base64::encode(bytes) })),
        )?;
        Ok(stored.as_str().unwrap_or(name).to_string())
    }

    /// For each name, whether Anki's media folder has it already
    /// (`getMediaFilesNames`, one `multi`).
    pub fn media_exist(&self, names: &[String]) -> Result<Vec<bool>, AnkiError> {
        if names.is_empty() {
            return Ok(Vec::new());
        }
        let actions: Vec<(&str, Option<Value>)> = names
            .iter()
            .map(|n| ("getMediaFilesNames", Some(json!({ "pattern": n }))))
            .collect();
        self.multi(&actions)?
            .into_iter()
            .zip(names)
            .map(|(found, name)| {
                let found = self.strings(found?, "getMediaFilesNames")?;
                Ok(found.iter().any(|f| f == name))
            })
            .collect()
    }

    /// `deleteMediaFile`.
    pub fn delete_media(&self, name: &str) -> Result<(), AnkiError> {
        self.call("deleteMediaFile", Some(json!({ "filename": name })))
            .map(drop)
    }

    /// `createModel` with one card template.
    pub fn create_model(&self, model: &NewModel<'_>) -> Result<(), AnkiError> {
        self.call(
            "createModel",
            Some(json!({
                "modelName": model.name,
                "inOrderFields": model.fields,
                "css": model.css,
                "isCloze": false,
                "cardTemplates": [{
                    "Name": model.template,
                    "Front": model.front,
                    "Back": model.back,
                }],
            })),
        )
        .map(drop)
    }

    /// `createDeck` (it returns the existing deck's id if there is one).
    pub fn create_deck(&self, name: &str) -> Result<(), AnkiError> {
        self.call("createDeck", Some(json!({ "deck": name })))
            .map(drop)
    }

    /// `guiBrowse`: opens Anki's browser on `query` (ANK-038). Some versions
    /// return nothing, so the result is not read.
    pub fn gui_browse(&self, query: &str) -> Result<(), AnkiError> {
        self.call("guiBrowse", Some(json!({ "query": query })))
            .map(drop)
    }
}

/// A note type to create.
#[derive(Debug, Clone, Copy)]
pub struct NewModel<'a> {
    /// Its name.
    pub name: &'a str,
    /// Its fields, in order.
    pub fields: &'a [&'a str],
    /// Its styling.
    pub css: &'a str,
    /// The name of its card template.
    pub template: &'a str,
    /// The front template.
    pub front: &'a str,
    /// The back template.
    pub back: &'a str,
}

/// A reply that cannot be read: for an action that changes Anki, whether it
/// was carried out is unknown (ANK-003).
fn unread(action: &str, idempotent: bool, e: AnkiError) -> AnkiError {
    if idempotent {
        e
    } else {
        AnkiError::CommitUnknown {
            action: action.to_string(),
        }
    }
}

/// `result` if `error` is null, else the typed error.
fn answer(action: &str, error: Value, result: Value) -> Result<Value, AnkiError> {
    match error {
        Value::Null => Ok(result),
        Value::String(message) => Err(AnkiError::from_anki(action, &message)),
        other => Err(AnkiError::Anki(other.to_string())),
    }
}

fn note_info(value: &Value) -> Option<NoteInfo> {
    let id = value.get("noteId")?.as_u64()?;
    let model = value.get("modelName")?.as_str()?.to_string();
    let mut fields: Vec<(u64, String, String)> = value
        .get("fields")?
        .as_object()?
        .iter()
        .map(|(name, f)| {
            (
                f.get("order").and_then(Value::as_u64).unwrap_or(u64::MAX),
                name.clone(),
                f.get("value")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            )
        })
        .collect();
    fields.sort();
    let tags = value
        .get("tags")
        .and_then(Value::as_array)
        .map(|t| {
            t.iter()
                .filter_map(|t| t.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    Some(NoteInfo {
        id,
        model,
        fields: fields.into_iter().map(|(_, k, v)| (k, v)).collect(),
        tags,
    })
}

/// reqwest's message without the address it adds, which the error gives
/// already.
fn without_url(e: &reqwest::Error) -> String {
    let mut source: Option<&dyn std::error::Error> = std::error::Error::source(e);
    let mut last = None;
    while let Some(s) = source {
        last = Some(s.to_string());
        source = s.source();
    }
    last.unwrap_or_else(|| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses() {
        for bad in ["", "127.0.0.1:8765", "ftp://x", "http://", "not a url"] {
            assert!(Client::new(bad, "").is_err(), "{bad:?}");
        }
        let c = Client::new(" http://127.0.0.1:8765 ", "").unwrap();
        assert_eq!(c.url(), "http://127.0.0.1:8765");
        assert!(c.is_loopback());
        for local in [
            "http://localhost:8765",
            "http://127.1.2.3",
            "http://[::1]:8765",
        ] {
            assert!(Client::new(local, "").unwrap().is_loopback(), "{local}");
        }
        for remote in ["http://192.168.1.2:8765", "https://anki.example.org"] {
            assert!(!Client::new(remote, "").unwrap().is_loopback(), "{remote}");
        }
        // a password in the address never reaches a message
        let c = Client::new("http://user:secret@127.0.0.1:8765/", "").unwrap();
        assert_eq!(c.url(), "http://127.0.0.1:8765");
        assert!(!format!("{c:?}").contains("secret"));
        for bad in [
            "htps://user:secret@anki.example.org",
            "https://user:secret@anki.example.org:87650",
            "user:secret@127.0.0.1:8765",
        ] {
            let e = Client::new(bad, "").unwrap_err();
            assert!(!e.to_string().contains("secret"), "{e}");
            assert!(
                e.to_string().contains("anki.example.org") || e.to_string().contains("127.0.0.1")
            );
        }
    }

    // T-ANK-001
    #[test]
    fn the_key_is_sent_only_when_set() {
        let without = Client::new("http://127.0.0.1:8765", "").unwrap();
        assert_eq!(
            serde_json::to_string(&without.body("deckNames", None)).unwrap(),
            r#"{"action":"deckNames","version":6}"#
        );
        let with = Client::new("http://127.0.0.1:8765", "k3y").unwrap();
        assert_eq!(
            serde_json::to_string(
                &with.body("modelFieldNames", Some(&json!({"modelName": "Lapis"})))
            )
            .unwrap(),
            r#"{"action":"modelFieldNames","key":"k3y","params":{"modelName":"Lapis"},"version":6}"#
        );
        assert!(!format!("{with:?}").contains("k3y"));
    }

    #[test]
    fn searches_escape_what_anki_reads_as_syntax() {
        assert_eq!(
            field_query("Expression", "食べ物"),
            r#""Expression:食べ物""#
        );
        assert_eq!(
            field_query("Word", r#"a"b*c_d\e"#),
            r#""Word:a\"b\*c\_d\\e""#
        );
        assert_eq!(deck_query(r#"Jp::"Mining""#), r#""deck:Jp::\"Mining\"""#);
    }

    #[test]
    fn notes_info_keeps_the_field_order() {
        let info = note_info(&json!({
            "noteId": 7, "modelName": "Lapis", "tags": ["a", "b"],
            "fields": {
                "Sentence": {"value": "s", "order": 2},
                "Expression": {"value": "e", "order": 0},
                "Reading": {"value": "r", "order": 1},
            }
        }))
        .unwrap();
        assert_eq!(info.id, 7);
        assert_eq!(
            info.fields,
            vec![
                ("Expression".to_string(), "e".to_string()),
                ("Reading".to_string(), "r".to_string()),
                ("Sentence".to_string(), "s".to_string()),
            ]
        );
        assert_eq!(info.tags, ["a", "b"]);
        assert_eq!(note_info(&json!({})), None, "a missing note is [{{}}]");
    }
}
