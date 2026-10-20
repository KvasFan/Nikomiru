// SPDX-License-Identifier: GPL-3.0-only

//! A fake AnkiConnect for the tests (feature `test-support`, [ADR 0007] §7):
//! an HTTP server on `127.0.0.1` that answers the legacy API from a small
//! collection kept in memory, records every request, and can answer what the
//! test queues instead: an HTTP status, a body that is not AnkiConnect's, a
//! connection closed before the reply (with or without applying the action)
//! or no reply at all.
//!
//! [ADR 0007]: ../../../docs/adr/0007-mining-to-anki.md

use std::collections::{BTreeMap, VecDeque};
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::Duration;

use serde_json::{Value, json};

/// The id of the first note the fake creates.
pub const FIRST_NOTE_ID: u64 = 1_760_000_000_000;

/// What the fake does with one request, instead of answering it.
#[derive(Debug, Clone, PartialEq)]
pub enum Behaviour {
    /// Answers this JSON with status 200.
    Json(Value),
    /// Answers this status with an empty body.
    Status(u16),
    /// Answers this body with status 200, as it is.
    Body(String),
    /// Reads the request and closes the connection without a reply.
    Close,
    /// Applies the request to the collection, then closes without a reply: the
    /// reply was lost after the change.
    ApplyThenClose,
    /// Reads the request and never replies.
    Hang,
}

/// A note type of the fake collection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Model {
    /// Its name.
    pub name: String,
    /// Its id.
    pub id: u64,
    /// Its fields, in order.
    pub fields: Vec<String>,
    /// Its styling.
    pub css: String,
    /// Its card templates: name, front, back.
    pub templates: Vec<(String, String, String)>,
}

/// A note of the fake collection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredNote {
    /// Its id.
    pub id: u64,
    /// Its deck.
    pub deck: String,
    /// Its note type.
    pub model: String,
    /// Its fields, in the note type's order.
    pub fields: Vec<(String, String)>,
    /// Its tags.
    pub tags: Vec<String>,
}

/// What the fake Anki holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Collection {
    /// Deck names.
    pub decks: Vec<String>,
    /// Note types.
    pub models: Vec<Model>,
    /// Notes.
    pub notes: Vec<StoredNote>,
    /// Media files by name.
    pub media: BTreeMap<String, Vec<u8>>,
    /// The queries of `guiBrowse`, in order.
    pub browsed: Vec<String>,
}

impl Default for Collection {
    /// A new profile: the deck `Default` and the note type `Basic`.
    fn default() -> Self {
        Self {
            decks: vec!["Default".to_string()],
            models: vec![Model {
                name: "Basic".to_string(),
                id: 1_000,
                fields: vec!["Front".to_string(), "Back".to_string()],
                css: String::new(),
                templates: vec![(
                    "Card 1".to_string(),
                    "{{Front}}".to_string(),
                    "{{Back}}".to_string(),
                )],
            }],
            notes: Vec::new(),
            media: BTreeMap::new(),
            browsed: Vec::new(),
        }
    }
}

#[derive(Default)]
struct Shared {
    collection: Mutex<Collection>,
    requests: Mutex<Vec<Value>>,
    script: Mutex<VecDeque<Behaviour>>,
    key: Mutex<String>,
    unsupported: Mutex<Vec<String>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The running fake; it stops when dropped.
pub struct FakeAnki {
    port: u16,
    shared: Arc<Shared>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for FakeAnki {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FakeAnki")
            .field("port", &self.port)
            .finish()
    }
}

impl FakeAnki {
    /// Starts a fake with a new profile ([`Collection::default`]).
    pub fn start() -> std::io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let port = listener.local_addr()?.port();
        let shared = Arc::new(Shared::default());
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let (shared, stop) = (shared.clone(), stop.clone());
            std::thread::Builder::new()
                .name("fake-anki".into())
                .spawn(move || {
                    for stream in listener.incoming() {
                        if stop.load(Ordering::SeqCst) {
                            break;
                        }
                        let Ok(stream) = stream else { continue };
                        let shared = shared.clone();
                        let _ = std::thread::Builder::new()
                            .name("fake-anki-conn".into())
                            .spawn(move || serve(&shared, stream));
                    }
                })?
        };
        Ok(Self {
            port,
            shared,
            stop,
            thread: Some(thread),
        })
    }

    /// Its address, for `[anki] url`.
    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// Requires this API key from now on.
    pub fn set_key(&self, key: &str) {
        *lock(&self.shared.key) = key.to_string();
    }

    /// Answers `unsupported action` to these actions, like an old AnkiConnect.
    pub fn unsupported(&self, actions: &[&str]) {
        *lock(&self.shared.unsupported) = actions.iter().map(|a| (*a).to_string()).collect();
    }

    /// Does `behaviour` with the next request, instead of answering it.
    pub fn then(&self, behaviour: Behaviour) {
        lock(&self.shared.script).push_back(behaviour);
    }

    /// The bodies of the requests so far (a body that is not JSON is a string).
    pub fn requests(&self) -> Vec<Value> {
        lock(&self.shared.requests).clone()
    }

    /// The actions of the requests so far, in order.
    pub fn actions(&self) -> Vec<String> {
        self.requests()
            .iter()
            .map(|r| r["action"].as_str().unwrap_or_default().to_string())
            .collect()
    }

    /// Forgets the requests so far.
    pub fn clear_requests(&self) {
        lock(&self.shared.requests).clear();
    }

    /// The collection, to look at or to prepare.
    pub fn collection(&self) -> MutexGuard<'_, Collection> {
        lock(&self.shared.collection)
    }
}

impl Drop for FakeAnki {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        // wakes the accept loop
        let _ = TcpStream::connect(("127.0.0.1", self.port));
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Reads one HTTP request: its body, or `None` with the status to answer.
fn read_request(stream: &mut TcpStream) -> Result<Vec<u8>, u16> {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 8192];
    let head_end = loop {
        if let Some(i) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
        if buffer.len() > 64 * 1024 {
            return Err(431);
        }
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return Err(400),
            Ok(n) => buffer.extend_from_slice(&chunk[..n]),
        }
    };
    let head = String::from_utf8_lossy(&buffer[..head_end]).to_lowercase();
    if head.contains("transfer-encoding: chunked") {
        return Err(400);
    }
    let length = head
        .lines()
        .find_map(|l| l.strip_prefix("content-length:"))
        .and_then(|v| v.trim().parse::<usize>().ok())
        .unwrap_or(0);
    if length > 64 << 20 {
        return Err(413);
    }
    let mut body = buffer[head_end..].to_vec();
    while body.len() < length {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return Err(400),
            Ok(n) => body.extend_from_slice(&chunk[..n]),
        }
    }
    body.truncate(length);
    Ok(body)
}

fn respond(stream: &mut TcpStream, status: u16, body: &str) {
    let reason = if status == 200 { "OK" } else { "Error" };
    let _ = write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.flush();
}

fn serve(shared: &Shared, mut stream: TcpStream) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(60)));
    let body = match read_request(&mut stream) {
        Ok(body) => body,
        Err(status) => return respond(&mut stream, status, ""),
    };
    let request: Value = serde_json::from_slice(&body)
        .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&body).into_owned()));
    lock(&shared.requests).push(request.clone());
    let behaviour = lock(&shared.script).pop_front();
    match behaviour {
        None => {
            let reply = answer(shared, &request);
            respond(&mut stream, 200, &reply.to_string());
        }
        Some(Behaviour::Json(v)) => respond(&mut stream, 200, &v.to_string()),
        Some(Behaviour::Status(status)) => respond(&mut stream, status, ""),
        Some(Behaviour::Body(text)) => respond(&mut stream, 200, &text),
        Some(Behaviour::Close) => {
            let _ = stream.shutdown(Shutdown::Both);
        }
        Some(Behaviour::ApplyThenClose) => {
            let _ = answer(shared, &request);
            let _ = stream.shutdown(Shutdown::Both);
        }
        Some(Behaviour::Hang) => {
            // until the client gives up
            let mut sink = [0u8; 64];
            while matches!(stream.read(&mut sink), Ok(n) if n > 0) {}
        }
    }
}

/// The `{result, error}` reply to one request.
fn answer(shared: &Shared, request: &Value) -> Value {
    let reply = |r: Result<Value, String>| match r {
        Ok(result) => json!({ "result": result, "error": null }),
        Err(error) => json!({ "result": null, "error": error }),
    };
    let key = lock(&shared.key).clone();
    if !key.is_empty() && request["key"].as_str() != Some(key.as_str()) {
        return reply(Err("valid api key must be provided".to_string()));
    }
    let action = request["action"].as_str().unwrap_or_default();
    if lock(&shared.unsupported).iter().any(|a| a == action) {
        return reply(Err("unsupported action".to_string()));
    }
    let params = &request["params"];
    if action == "multi" {
        let replies: Vec<Value> = params["actions"]
            .as_array()
            .map(|a| a.iter().map(|child| answer(shared, child)).collect())
            .unwrap_or_default();
        return reply(Ok(Value::Array(replies)));
    }
    reply(lock(&shared.collection).run(action, params))
}

impl Collection {
    fn model(&self, name: &str) -> Result<&Model, String> {
        self.models
            .iter()
            .find(|m| m.name == name)
            .ok_or_else(|| format!("model was not found: {name}"))
    }

    fn in_deck(note: &StoredNote, deck: &str) -> bool {
        note.deck == deck || note.deck.starts_with(&format!("{deck}::"))
    }

    /// Whether `note` (from params) can be added: the checks of `addNote`.
    fn check(&self, note: &Value, allow_duplicate: bool) -> Result<(), String> {
        let deck = note["deckName"].as_str().unwrap_or_default();
        let model = self.model(note["modelName"].as_str().unwrap_or_default())?;
        if !self.decks.iter().any(|d| d == deck) {
            return Err(format!("deck was not found: {deck}"));
        }
        let first = model.fields.first().cloned().unwrap_or_default();
        let value = note["fields"][&first].as_str().unwrap_or_default();
        if value.trim().is_empty() {
            return Err("cannot create note because it is empty".to_string());
        }
        let options = &note["options"];
        if allow_duplicate || options["allowDuplicate"] == true {
            return Ok(());
        }
        let collection = options["duplicateScope"] == "collection";
        let scope_deck = options["duplicateScopeOptions"]["deckName"]
            .as_str()
            .unwrap_or(deck);
        let duplicate = self.notes.iter().any(|n| {
            (collection || Self::in_deck(n, scope_deck))
                && n.fields.first().is_some_and(|(_, v)| v == value)
        });
        if duplicate {
            return Err(crate::client::DUPLICATE.to_string());
        }
        Ok(())
    }

    fn run(&mut self, action: &str, p: &Value) -> Result<Value, String> {
        let text = |v: &Value| v.as_str().unwrap_or_default().to_string();
        match action {
            "version" => Ok(json!(6)),
            "deckNames" => Ok(json!(self.decks)),
            "modelNames" => Ok(json!(
                self.models.iter().map(|m| &m.name).collect::<Vec<_>>()
            )),
            "modelNamesAndIds" => Ok(Value::Object(
                self.models
                    .iter()
                    .map(|m| (m.name.clone(), json!(m.id)))
                    .collect(),
            )),
            "modelFieldNames" => Ok(json!(self.model(&text(&p["modelName"]))?.fields)),
            "createDeck" => {
                let deck = text(&p["deck"]);
                if !self.decks.contains(&deck) {
                    self.decks.push(deck.clone());
                }
                let id = self.decks.iter().position(|d| *d == deck).unwrap_or(0);
                Ok(json!(2_000 + id))
            }
            "createModel" => {
                let name = text(&p["modelName"]);
                if self.models.iter().any(|m| m.name == name) {
                    return Err("Model name already exists".to_string());
                }
                let model = Model {
                    name,
                    id: 1_000 + self.models.len() as u64,
                    fields: p["inOrderFields"]
                        .as_array()
                        .map(|f| f.iter().map(text).collect())
                        .unwrap_or_default(),
                    css: text(&p["css"]),
                    templates: p["cardTemplates"]
                        .as_array()
                        .map(|t| {
                            t.iter()
                                .map(|t| (text(&t["Name"]), text(&t["Front"]), text(&t["Back"])))
                                .collect()
                        })
                        .unwrap_or_default(),
                };
                let reply = json!({ "name": model.name, "id": model.id });
                self.models.push(model);
                Ok(reply)
            }
            "canAddNotesWithErrorDetail" => Ok(Value::Array(
                p["notes"]
                    .as_array()
                    .map(|notes| {
                        notes
                            .iter()
                            .map(|n| match self.check(n, false) {
                                Ok(()) => json!({ "canAdd": true }),
                                Err(e) => json!({ "canAdd": false, "error": e }),
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            )),
            "addNote" => {
                let note = &p["note"];
                self.check(note, false)?;
                let model = self.model(&text(&note["modelName"]))?.clone();
                let id = FIRST_NOTE_ID + self.notes.len() as u64;
                self.notes.push(StoredNote {
                    id,
                    deck: text(&note["deckName"]),
                    model: model.name.clone(),
                    fields: model
                        .fields
                        .iter()
                        .map(|f| (f.clone(), text(&note["fields"][f])))
                        .collect(),
                    tags: note["tags"]
                        .as_array()
                        .map(|t| t.iter().map(text).collect())
                        .unwrap_or_default(),
                });
                Ok(json!(id))
            }
            "findNotes" => Ok(json!(self.find(&text(&p["query"])))),
            "notesInfo" => Ok(Value::Array(
                p["notes"]
                    .as_array()
                    .map(|ids| {
                        ids.iter()
                            .map(|id| {
                                self.notes
                                    .iter()
                                    .find(|n| Some(n.id) == id.as_u64())
                                    .map_or_else(|| json!({}), info)
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            )),
            "updateNoteFields" => {
                let id = p["note"]["id"].as_u64().unwrap_or_default();
                let note = self
                    .notes
                    .iter_mut()
                    .find(|n| n.id == id)
                    .ok_or_else(|| format!("Note was not found: {id}"))?;
                if let Some(fields) = p["note"]["fields"].as_object() {
                    for (name, value) in &mut note.fields {
                        if let Some(v) = fields.get(name.as_str()) {
                            *value = text(v);
                        }
                    }
                }
                Ok(Value::Null)
            }
            "addTags" => {
                let tags: Vec<String> = text(&p["tags"])
                    .split_whitespace()
                    .map(str::to_string)
                    .collect();
                for id in p["notes"].as_array().into_iter().flatten() {
                    if let Some(n) = self.notes.iter_mut().find(|n| Some(n.id) == id.as_u64()) {
                        for t in &tags {
                            if !n.tags.contains(t) {
                                n.tags.push(t.clone());
                            }
                        }
                    }
                }
                Ok(Value::Null)
            }
            "storeMediaFile" => {
                let name = text(&p["filename"]);
                if let Some(data) = p["data"].as_str() {
                    let bytes = crate::base64::decode(data)
                        .ok_or_else(|| "Incorrect padding".to_string())?;
                    self.media.insert(name.clone(), bytes);
                    Ok(json!(name))
                } else {
                    // like a sandboxed Anki, which cannot read Nikomiru's files
                    Err(format!(
                        "[Errno 2] No such file or directory: '{}'",
                        text(&p["path"])
                    ))
                }
            }
            "getMediaFilesNames" => {
                let pattern = text(&p["pattern"]);
                Ok(json!(
                    self.media
                        .keys()
                        .filter(|name| glob(&pattern, name))
                        .collect::<Vec<_>>()
                ))
            }
            "deleteMediaFile" => {
                self.media.remove(&text(&p["filename"]));
                Ok(Value::Null)
            }
            "guiBrowse" => {
                let query = text(&p["query"]);
                let found = self.find(&query);
                self.browsed.push(query);
                Ok(json!(found))
            }
            _ => Err("unsupported action".to_string()),
        }
    }

    /// The notes matching the searches Nikomiru sends: terms joined by spaces
    /// (all must match), a term being `deck:`, `nid:`, `dupe:`, `Field:value`
    /// or a group `(a OR b)`.
    fn find(&self, query: &str) -> Vec<u64> {
        let terms = tokens(query);
        self.notes
            .iter()
            .filter(|n| {
                terms
                    .iter()
                    .all(|alternatives| alternatives.iter().any(|t| self.term_matches(n, t)))
            })
            .map(|n| n.id)
            .collect()
    }

    fn term_matches(&self, note: &StoredNote, term: &str) -> bool {
        if let Some(deck) = term.strip_prefix("deck:") {
            return Self::in_deck(note, deck);
        }
        if let Some(ids) = term.strip_prefix("nid:") {
            return ids.split(',').any(|id| id.parse() == Ok(note.id));
        }
        if let Some(rest) = term.strip_prefix("dupe:") {
            let Some((mid, value)) = rest.split_once(',') else {
                return false;
            };
            let model = self.models.iter().find(|m| m.name == note.model);
            return model.is_some_and(|m| mid.parse() == Ok(m.id))
                && note.fields.first().is_some_and(|(_, v)| v == value);
        }
        match term.split_once(':') {
            Some((field, value)) => note
                .fields
                .iter()
                .any(|(name, v)| name.eq_ignore_ascii_case(field) && v == value),
            None => false,
        }
    }
}

fn info(note: &StoredNote) -> Value {
    json!({
        "noteId": note.id,
        "modelName": note.model,
        "tags": note.tags,
        "fields": note
            .fields
            .iter()
            .enumerate()
            .map(|(order, (name, value))| (name.clone(), json!({ "value": value, "order": order })))
            .collect::<serde_json::Map<_, _>>(),
    })
}

/// The search split into terms (quotes and `\` escapes removed); each term
/// is a list of alternatives (`(a OR b)`).
fn tokens(query: &str) -> Vec<Vec<String>> {
    let mut words = Vec::new();
    let mut word = String::new();
    let (mut quoted, mut chars) = (false, query.chars());
    while let Some(c) = chars.next() {
        match c {
            '\\' => word.extend(chars.next()),
            '"' => quoted = !quoted,
            '(' | ')' if !quoted => {
                if !word.is_empty() {
                    words.push(std::mem::take(&mut word));
                }
                words.push(c.to_string());
            }
            c if c.is_whitespace() && !quoted => {
                if !word.is_empty() {
                    words.push(std::mem::take(&mut word));
                }
            }
            c => word.push(c),
        }
    }
    if !word.is_empty() {
        words.push(word);
    }
    let mut terms = Vec::new();
    let mut words = words.into_iter();
    while let Some(w) = words.next() {
        if w == "(" {
            let group: Vec<String> = words
                .by_ref()
                .take_while(|w| w != ")")
                .filter(|w| w != "OR")
                .collect();
            terms.push(group);
        } else {
            terms.push(vec![w]);
        }
    }
    terms
}

/// `*` and `?` globbing, as `getMediaFilesNames` does.
fn glob(pattern: &str, name: &str) -> bool {
    fn go(p: &[char], n: &[char]) -> bool {
        match (p.first(), n.first()) {
            (None, None) => true,
            (Some('*'), _) => go(&p[1..], n) || (!n.is_empty() && go(p, &n[1..])),
            (Some('?'), Some(_)) => go(&p[1..], &n[1..]),
            (Some(a), Some(b)) if a == b => go(&p[1..], &n[1..]),
            _ => false,
        }
    }
    let p: Vec<char> = pattern.chars().collect();
    let n: Vec<char> = name.chars().collect();
    go(&p, &n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn searches() {
        assert_eq!(
            tokens(r#""deck:Jp::Mining" "Expression:a\"b" (dupe:1,x OR dupe:2,x)"#),
            vec![
                vec!["deck:Jp::Mining".to_string()],
                vec!["Expression:a\"b".to_string()],
                vec!["dupe:1,x".to_string(), "dupe:2,x".to_string()],
            ]
        );
        assert!(glob("nikomiru_*.jpg", "nikomiru_ab.jpg"));
        assert!(glob("a?c", "abc"));
        assert!(!glob("nikomiru_ab.jpg", "nikomiru_ab.jpgx"));
    }
}
