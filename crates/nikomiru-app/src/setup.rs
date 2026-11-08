// SPDX-License-Identifier: GPL-3.0-only

//! The `setup` worker (INS-002…INS-006, D-48): the dictionaries of the
//! catalogue downloaded for the session to import, and the audio libraries
//! of Fushi's recommended pack, read by range: the pack's `fushi.db` says
//! which library is which, the one ticked is downloaded alone, checked, and
//! opened as a Local Audio Server database (AUD-003) before it is handed on.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError, mpsc};
use std::time::Duration;

use nikomiru_core::Cancel;
use nikomiru_online::OnlineError;
use nikomiru_online::http::Http;
use nikomiru_online::setup::catalog::Catalog;
use nikomiru_online::setup::fetch::{MANIFEST_URLS, Pack, Url, download_file, manifest};
use nikomiru_online::setup::pack::Entry;
use nikomiru_ui::i18n::Translator;
use nikomiru_ui::setup::{Plan, SetupEvent, SetupService};

/// What the worker needs.
#[derive(Clone)]
pub struct Parts {
    /// `[network] proxy` (SET-007).
    pub proxy: String,
    /// Where dictionaries are downloaded (`$XDG_CACHE_HOME/nikomiru/setup`).
    pub cache_dir: PathBuf,
    /// Where audio libraries go (`$XDG_DATA_HOME/nikomiru/audio`).
    pub audio_dir: PathBuf,
    /// Where the pack's manifest is read, in order.
    pub manifest_urls: Vec<String>,
    /// The interface strings, for the messages.
    pub translator: Translator,
    /// The catalogue: the one shipped with Nikomiru, but in the tests.
    pub catalog: Catalog,
}

impl std::fmt::Debug for Parts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Parts")
            .field("cache_dir", &self.cache_dir)
            .field("audio_dir", &self.audio_dir)
            .finish_non_exhaustive()
    }
}

impl Parts {
    /// The manifest addresses of Fushi's pack (INS-003).
    pub fn manifest_urls() -> Vec<String> {
        MANIFEST_URLS.iter().map(|u| (*u).to_string()).collect()
    }
}

/// The free space a library leaves at least (INS-004).
const SPARE: u64 = 1 << 30;

/// The largest `fushi.db` read.
const MAX_LABELS_DB: u64 = 32 << 20;

type Events = Box<dyn Fn(SetupEvent) + Send + Sync>;

/// The worker: one plan at a time, on a thread of its own.
pub struct Setup {
    jobs: Option<mpsc::Sender<(Plan, Events, Cancel)>>,
    running: Arc<Mutex<Cancel>>,
}

impl std::fmt::Debug for Setup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Setup").finish_non_exhaustive()
    }
}

impl Setup {
    /// Starts the worker.
    pub fn start(parts: Parts) -> Self {
        let (jobs, rx) = mpsc::channel::<(Plan, Events, Cancel)>();
        let spawned = std::thread::Builder::new()
            .name("nikomiru-setup".into())
            .spawn(move || {
                for (plan, events, cancel) in rx {
                    run(&parts, &plan, &events, &cancel);
                    events(SetupEvent::Finished);
                }
            });
        let jobs = match spawned {
            Ok(_) => Some(jobs),
            Err(e) => {
                tracing::error!("cannot start the setup thread: {e}");
                None
            }
        };
        Self {
            jobs,
            running: Arc::new(Mutex::new(Cancel::new())),
        }
    }
}

impl SetupService for Setup {
    fn install(&self, plan: Plan, events: Events) {
        let cancel = Cancel::new();
        *self.running.lock().unwrap_or_else(PoisonError::into_inner) = cancel.clone();
        match &self.jobs {
            Some(jobs) => {
                if let Err(mpsc::SendError((_, events, _))) = jobs.send((plan, events, cancel)) {
                    events(SetupEvent::Finished);
                }
            }
            None => events(SetupEvent::Finished),
        }
    }

    fn stop(&self) {
        self.running
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .cancel();
    }
}

fn message(parts: &Parts, e: &OnlineError) -> String {
    parts.translator.error(e)
}

/// Runs a plan: the dictionaries, then the libraries.
fn run(parts: &Parts, plan: &Plan, events: &Events, cancel: &Cancel) {
    let http = match Http::with_timeouts(
        &parts.proxy,
        Duration::from_secs(10),
        Duration::from_secs(300),
    ) {
        Ok(http) => http,
        Err(e) => {
            let text = message(parts, &e);
            for id in plan.dictionaries.iter().chain(&plan.audio) {
                events(SetupEvent::Failed {
                    id: id.clone(),
                    message: text.clone(),
                });
            }
            return;
        }
    };
    for id in &plan.dictionaries {
        let Some(d) = parts.catalog.dictionaries.iter().find(|d| d.id == *id) else {
            continue;
        };
        let failed = |message: String| SetupEvent::Failed {
            id: id.clone(),
            message,
        };
        let Ok(url) = Url::parse(&d.url) else {
            events(failed(format!("{}: {}", d.name, d.url)));
            continue;
        };
        let dest = parts.cache_dir.join(format!("{id}.zip"));
        let total = d.size_bytes;
        let result = download_file(
            &http,
            &url,
            d.sha256.as_deref(),
            &dest,
            cancel,
            &mut |done| {
                events(SetupEvent::Progress {
                    id: id.clone(),
                    done,
                    total: total.max(done),
                });
            },
        );
        match result {
            Ok(path) => events(SetupEvent::Dictionary {
                id: id.clone(),
                path,
            }),
            Err(e) => events(failed(message(parts, &e))),
        }
    }
    if !plan.audio.is_empty() {
        audio(parts, &http, plan, events, cancel);
    }
}

/// The libraries of the plan, from Fushi's pack.
fn audio(parts: &Parts, http: &Http, plan: &Plan, events: &Events, cancel: &Cancel) {
    let fail_all = |text: String| {
        for id in &plan.audio {
            events(SetupEvent::Failed {
                id: id.clone(),
                message: text.clone(),
            });
        }
    };
    let urls: Vec<Url> = parts
        .manifest_urls
        .iter()
        .filter_map(|u| Url::parse(u).ok())
        .collect();
    let opened = manifest(http, &urls).and_then(|m| {
        let pack = Pack::new(http, m);
        let entries = pack.directory(cancel)?;
        Ok((pack, entries))
    });
    let (pack, entries) = match opened {
        Ok(opened) => opened,
        Err(e) => return fail_all(message(parts, &e)),
    };
    let labels = labels(parts, &pack, &entries, cancel);
    for id in &plan.audio {
        let Some(library) = parts.catalog.audio.iter().find(|a| a.id == *id) else {
            continue;
        };
        let failed = |message: String| SetupEvent::Failed {
            id: id.clone(),
            message,
        };
        let Some(entry) = library_entry(&entries, &labels, &library.name) else {
            events(failed(parts.translator.tr(
                "error-setup-no-library",
                &[("name", library.name.clone())],
            )));
            continue;
        };
        let dest = parts.audio_dir.join(&library.name);
        if let Err(text) = room(parts, &dest, entry.size) {
            events(failed(text));
            continue;
        }
        let total = entry.size;
        let result = pack.download(entry, &dest, cancel, &mut |done| {
            events(SetupEvent::Progress {
                id: id.clone(),
                done,
                total,
            });
        });
        match result {
            Ok(path) => match nikomiru_dict::local_audio::LocalAudio::open(&path) {
                Ok(_) => events(SetupEvent::Audio {
                    id: id.clone(),
                    path,
                }),
                Err(e) => {
                    let _ = std::fs::remove_file(&path);
                    events(failed(
                        parts
                            .translator
                            .tr("error-setup-not-audio", &[("reason", e.to_string())]),
                    ));
                }
            },
            Err(e) => events(failed(message(parts, &e))),
        }
    }
}

/// Enough free space for `size` bytes more at `dest` (INS-004), or the
/// message saying not.
fn room(parts: &Parts, dest: &Path, size: u64) -> Result<(), String> {
    let dir = dest.parent().unwrap_or(dest);
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let there =
        std::fs::metadata(nikomiru_online::setup::fetch::part_path(dest)).map_or(0, |m| m.len());
    let need = size.saturating_sub(there) + SPARE;
    match crate::platform::free_space(dir) {
        Some(free) if free < need => Err(parts.translator.tr(
            "error-setup-space",
            &[("need", bytes_text(need)), ("free", bytes_text(free))],
        )),
        _ => Ok(()),
    }
}

fn bytes_text(bytes: u64) -> String {
    format!("{:.1} GB", bytes as f64 / 1e9)
}

/// The names the pack's `fushi.db` gives its audio libraries, by the file
/// name of each (`local_audio_….db` → `android.db`); none when it cannot be
/// read.
fn labels(
    parts: &Parts,
    pack: &Pack<'_>,
    entries: &[Entry],
    cancel: &Cancel,
) -> HashMap<String, String> {
    let Some(db) = entries.iter().find(|e| e.name == "fushi.db") else {
        return HashMap::new();
    };
    let bytes = match pack.small(db, MAX_LABELS_DB, cancel) {
        Ok(bytes) => bytes,
        Err(e) => {
            tracing::warn!("pack's fushi.db: {e}");
            return HashMap::new();
        }
    };
    let file = parts.cache_dir.join("fushi.db");
    let read = std::fs::create_dir_all(&parts.cache_dir)
        .and_then(|()| std::fs::write(&file, &bytes))
        .map_err(|e| e.to_string())
        .and_then(|()| labels_of(&file).map_err(|e| e.to_string()));
    let _ = std::fs::remove_file(&file);
    read.unwrap_or_else(|e| {
        tracing::warn!("pack's fushi.db: {e}");
        HashMap::new()
    })
}

/// `preferences.local_audio_dbs` of a Fushi database: `s:` and a JSON list
/// of `{"path": …, "displayName": …}`.
pub fn labels_of(
    file: &Path,
) -> Result<HashMap<String, String>, nikomiru_dict::sqlite::SqliteError> {
    use nikomiru_dict::sqlite::{Database, Value};
    let db = Database::open(file)?;
    let Some(table) = db
        .schema()?
        .into_iter()
        .find(|t| t.kind == "table" && t.name == "preferences")
    else {
        return Ok(HashMap::new());
    };
    let columns = nikomiru_dict::sqlite::columns(&table.sql);
    let at = |name: &str| columns.iter().position(|(c, _)| c == name);
    let (Some(key), Some(value)) = (at("key"), at("value")) else {
        return Ok(HashMap::new());
    };
    let mut text = None;
    db.scan(table.root, &mut |_, row| {
        if row.get(key).and_then(Value::as_text) == Some("local_audio_dbs") {
            text = row.get(value).and_then(Value::as_text).map(str::to_string);
            return false;
        }
        true
    })?;
    let mut labels = HashMap::new();
    let json = text.as_deref().map(|t| t.strip_prefix("s:").unwrap_or(t));
    let list: Vec<serde_json::Value> = json
        .and_then(|t| serde_json::from_str(t).ok())
        .unwrap_or_default();
    for item in list {
        let (Some(path), Some(name)) = (item["path"].as_str(), item["displayName"].as_str()) else {
            continue;
        };
        let file = path.rsplit(['/', '\\']).next().unwrap_or(path);
        labels.insert(file.to_string(), name.to_string());
    }
    Ok(labels)
}

/// The entry of the library `name` (`android.db`): by the label of
/// `fushi.db`; without labels, the Japanese one is the largest.
fn library_entry<'e>(
    entries: &'e [Entry],
    labels: &HashMap<String, String>,
    name: &str,
) -> Option<&'e Entry> {
    let libraries: Vec<&Entry> = entries
        .iter()
        .filter(|e| e.name.starts_with("localAudio/") && e.name.ends_with(".db"))
        .collect();
    let file = |e: &Entry| e.name.rsplit('/').next().unwrap_or_default().to_string();
    if !labels.is_empty() {
        return libraries
            .into_iter()
            .find(|e| labels.get(&file(e)).is_some_and(|l| l == name));
    }
    let mut by_size = libraries;
    by_size.sort_by_key(|e| std::cmp::Reverse(e.size));
    match name {
        "android.db" => by_size.first().copied(),
        "android_english.db" => by_size.get(1).copied(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, size: u64) -> Entry {
        Entry {
            name: name.into(),
            method: 0,
            crc32: 0,
            compressed: size,
            size,
            header: 0,
        }
    }

    // T-INS-003: which library is which, by the labels or by size
    #[test]
    fn the_libraries_are_told_apart() {
        let entries = [
            entry("fushi.db", 10),
            entry("localAudio/local_audio_1.db", 6_000),
            entry("localAudio/local_audio_2.db", 1_000),
            entry("dictionaryResources/x/blobs.bin", 9_000),
        ];
        let labels = HashMap::from([
            (
                "local_audio_1.db".to_string(),
                "android_english.db".to_string(),
            ),
            ("local_audio_2.db".to_string(), "android.db".to_string()),
        ]);
        let pick = |labels: &HashMap<String, String>, name: &str| {
            library_entry(&entries, labels, name).map(|e| e.name.clone())
        };
        assert_eq!(
            pick(&labels, "android.db").as_deref(),
            Some("localAudio/local_audio_2.db")
        );
        let none = HashMap::new();
        assert_eq!(
            pick(&none, "android.db").as_deref(),
            Some("localAudio/local_audio_1.db")
        );
        assert_eq!(
            pick(&none, "android_english.db").as_deref(),
            Some("localAudio/local_audio_2.db")
        );
        assert_eq!(pick(&labels, "other.db"), None);
    }
}
