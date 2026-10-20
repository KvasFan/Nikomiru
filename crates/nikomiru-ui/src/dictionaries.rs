// SPDX-License-Identifier: GPL-3.0-only

//! The dictionaries page apart from its drawing (DIC-016, DIC-030…DIC-034): the list of
//! registered dictionaries and the import of a zip, with the update offer
//! for a new revision, its progress and its cancellation. The work is done
//! by a [`DictionaryService`] of the application.

use std::path::{Path, PathBuf};

use nikomiru_core::Cancel;
use nikomiru_dict::import::Progress;
use nikomiru_library::{Check, Collapse, DictionaryRecord};
use serde_json::{Value, json};

use crate::popup::Reply;

/// A finished lookup or import; may run on any thread.
pub type Done<T> = Box<dyn FnOnce(Result<T, String>) + Send>;

/// The dictionaries, from the application. Errors are already translated.
pub trait DictionaryService: Send {
    /// Looks `text` up away from the UI thread; `done` gets the popup content.
    fn lookup(&self, text: String, done: Done<Reply>);
    /// The registered dictionaries, in order.
    fn list(&self) -> Result<Vec<(String, DictionaryRecord)>, String>;
    /// What importing `zip` would do (DIC-016).
    fn check(&self, zip: &Path) -> Result<Check, String>;
    /// Imports `zip` away from the UI thread, replacing `replace` (an update);
    /// `done` gets the new id.
    fn import(
        &self,
        zip: PathBuf,
        replace: Option<String>,
        cancel: Cancel,
        progress: Box<dyn Fn(Progress) + Send>,
        done: Done<String>,
    );
    /// Enables or disables a dictionary (DIC-032).
    fn set_enabled(&self, id: &str, enabled: bool) -> Result<(), String>;
    /// Puts the dictionaries in this order (DIC-031).
    fn reorder(&self, ids: &[String]) -> Result<(), String>;
    /// Deletes a dictionary (DIC-021, DIC-033).
    fn delete(&self, id: &str) -> Result<(), String>;
    /// Renames a dictionary for display; `None` restores the title (DIC-033).
    fn rename(&self, id: &str, name: Option<String>) -> Result<(), String>;
    /// Sets the popup state of a dictionary's block (DIC-034).
    fn set_collapse(&self, id: &str, collapse: Collapse) -> Result<(), String>;
}

/// Where the import of a zip is.
#[derive(Debug, Default)]
pub enum Import {
    /// Nothing running.
    #[default]
    Idle,
    /// Another revision is installed: the user confirms the update.
    Confirm {
        /// The zip.
        zip: PathBuf,
        /// The installed dictionary.
        id: String,
        /// Its title.
        title: String,
        /// Its revision.
        installed: String,
    },
    /// Running.
    Running {
        /// The zip's file name.
        file: String,
        /// The last progress.
        progress: Progress,
        /// Stops it.
        cancel: Cancel,
        /// Matches the progress and the end to this import.
        generation: u64,
    },
}

/// What the session must do after a page operation.
#[derive(Debug)]
pub enum Effect {
    /// Show a message (already translated by the caller from this id and
    /// these arguments).
    Message(&'static str, Vec<(&'static str, String)>),
    /// Show an error, already translated.
    Error(String),
    /// Start the import of `zip` (replacing `replace`) with this generation.
    Start {
        /// The zip.
        zip: PathBuf,
        /// The dictionary it updates.
        replace: Option<String>,
        /// Matches the progress and the end.
        generation: u64,
        /// Stops it.
        cancel: Cancel,
    },
}

impl PartialEq for Effect {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Effect::Message(a, x), Effect::Message(b, y)) => a == b && x == y,
            (Effect::Error(a), Effect::Error(b)) => a == b,
            (
                Effect::Start {
                    zip: a,
                    replace: x,
                    generation: g,
                    ..
                },
                Effect::Start {
                    zip: b,
                    replace: y,
                    generation: h,
                    ..
                },
            ) => a == b && x == y && g == h,
            _ => false,
        }
    }
}

/// The page state.
#[derive(Debug, Default)]
pub struct DictionaryPage {
    /// The registered dictionaries, in order.
    pub rows: Vec<(String, DictionaryRecord)>,
    /// The import.
    pub import: Import,
    generation: u64,
    /// Changes whenever the page changes, so the UI rebuilds it.
    pub version: u64,
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

impl DictionaryPage {
    /// Reads the list again.
    pub fn refresh(&mut self, service: &dyn DictionaryService) -> Vec<Effect> {
        self.version += 1;
        match service.list() {
            Ok(rows) => {
                self.rows = rows;
                Vec::new()
            }
            Err(e) => vec![Effect::Error(e)],
        }
    }

    /// The user chose a zip (dialog or drop).
    pub fn choose(&mut self, service: &dyn DictionaryService, zip: PathBuf) -> Vec<Effect> {
        if !matches!(self.import, Import::Idle) {
            return vec![Effect::Message("dict-import-busy", Vec::new())];
        }
        match service.check(&zip) {
            Err(e) => vec![Effect::Error(e)],
            Ok(Check::SameRevision { id }) => {
                let title = self.title_of(&id);
                vec![Effect::Message(
                    "dict-same-revision",
                    vec![("title", title)],
                )]
            }
            Ok(Check::Update { id, installed }) => {
                let title = self.title_of(&id);
                self.import = Import::Confirm {
                    zip,
                    id,
                    title,
                    installed,
                };
                self.version += 1;
                Vec::new()
            }
            Ok(Check::New) => vec![self.start(zip, None)],
        }
    }

    fn title_of(&self, id: &str) -> String {
        self.rows
            .iter()
            .find(|(i, _)| i == id)
            .map(|(_, r)| r.name().to_string())
            .unwrap_or_default()
    }

    fn start(&mut self, zip: PathBuf, replace: Option<String>) -> Effect {
        self.generation += 1;
        let cancel = Cancel::new();
        self.import = Import::Running {
            file: file_name(&zip),
            progress: Progress::default(),
            cancel: cancel.clone(),
            generation: self.generation,
        };
        self.version += 1;
        Effect::Start {
            zip,
            replace,
            generation: self.generation,
            cancel,
        }
    }

    /// The answer to the update offer.
    pub fn confirm(&mut self, update: bool) -> Vec<Effect> {
        let Import::Confirm { zip, id, .. } = std::mem::take(&mut self.import) else {
            return Vec::new();
        };
        self.version += 1;
        if update {
            vec![self.start(zip, Some(id))]
        } else {
            Vec::new()
        }
    }

    /// Stops the running import.
    pub fn cancel(&mut self) {
        if let Import::Running { cancel, .. } = &self.import {
            cancel.cancel();
        }
    }

    /// Progress of import `generation`.
    pub fn progress(&mut self, generation: u64, p: Progress) {
        if let Import::Running {
            progress,
            generation: g,
            ..
        } = &mut self.import
            && *g == generation
        {
            *progress = p;
            self.version += 1;
        }
    }

    /// Import `generation` ended.
    pub fn finished(
        &mut self,
        service: &dyn DictionaryService,
        generation: u64,
        result: Result<String, String>,
    ) -> Vec<Effect> {
        if !matches!(&self.import, Import::Running { generation: g, .. } if *g == generation) {
            return Vec::new();
        }
        let file = match std::mem::take(&mut self.import) {
            Import::Running { file, .. } => file,
            _ => String::new(),
        };
        let mut effects = self.refresh(service);
        match result {
            Ok(id) => {
                let title = self.title_of(&id);
                effects.push(Effect::Message("dict-imported", vec![("title", title)]));
            }
            Err(e) => effects.push(Effect::Error(format!("{file}: {e}"))),
        }
        effects
    }

    /// Moves a dictionary up (`-1`) or down (`+1`) in the order.
    pub fn move_by(
        &mut self,
        service: &dyn DictionaryService,
        id: &str,
        delta: i32,
    ) -> Vec<Effect> {
        let Some(at) = self.rows.iter().position(|(i, _)| i == id) else {
            return Vec::new();
        };
        let to = (at as i64 + i64::from(delta)).clamp(0, self.rows.len() as i64 - 1) as usize;
        if to == at {
            return Vec::new();
        }
        let mut ids: Vec<String> = self.rows.iter().map(|(i, _)| i.clone()).collect();
        let moved = ids.remove(at);
        ids.insert(to, moved);
        self.after(service, service.reorder(&ids))
    }

    fn after(
        &mut self,
        service: &dyn DictionaryService,
        result: Result<(), String>,
    ) -> Vec<Effect> {
        let mut effects = self.refresh(service);
        if let Err(e) = result {
            effects.push(Effect::Error(e));
        }
        effects
    }

    /// Enables or disables.
    pub fn set_enabled(
        &mut self,
        service: &dyn DictionaryService,
        id: &str,
        enabled: bool,
    ) -> Vec<Effect> {
        let r = service.set_enabled(id, enabled);
        self.after(service, r)
    }

    /// Deletes (the page asked for confirmation).
    pub fn delete(&mut self, service: &dyn DictionaryService, id: &str) -> Vec<Effect> {
        let r = service.delete(id);
        self.after(service, r)
    }

    /// Renames; an empty name restores the title.
    pub fn rename(&mut self, service: &dyn DictionaryService, id: &str, name: &str) -> Vec<Effect> {
        let name = name.trim();
        let r = service.rename(id, (!name.is_empty()).then(|| name.to_string()));
        self.after(service, r)
    }

    /// Sets the block state: `automatic`, `expanded` or `collapsed`.
    pub fn set_collapse(
        &mut self,
        service: &dyn DictionaryService,
        id: &str,
        state: &str,
    ) -> Vec<Effect> {
        let collapse = match state {
            "expanded" => Collapse::Expanded,
            "collapsed" => Collapse::Collapsed,
            _ => Collapse::Automatic,
        };
        let r = service.set_collapse(id, collapse);
        self.after(service, r)
    }

    /// The page as JSON, for the test harness's `state`.
    pub fn json(&self) -> Value {
        let rows: Vec<Value> = self
            .rows
            .iter()
            .map(|(id, r)| {
                let mut types = Vec::new();
                for (on, name) in [
                    (r.kinds.terms, "terms"),
                    (r.kinds.kanji, "kanji"),
                    (r.kinds.frequency, "frequency"),
                    (r.kinds.pitch, "pitch"),
                ] {
                    if on {
                        types.push(name);
                    }
                }
                json!({
                    "id": id,
                    "name": r.name(),
                    "title": r.title,
                    "renamed": r.display_name.is_some(),
                    "revision": r.revision,
                    "enabled": r.enabled,
                    "types": types,
                    "terms": r.counts.terms,
                    "kanji": r.counts.kanji,
                    "frequencies": r.counts.frequencies + r.counts.kanji_meta,
                    "pitches": r.counts.pitches,
                    "size": r.size,
                    "importedAt": r.imported_at,
                    "collapse": match r.collapse {
                        Collapse::Automatic => "automatic",
                        Collapse::Expanded => "expanded",
                        Collapse::Collapsed => "collapsed",
                    },
                })
            })
            .collect();
        let import = match &self.import {
            Import::Idle => json!({"state": "idle"}),
            Import::Confirm {
                title,
                installed,
                zip,
                ..
            } => json!({
                "state": "confirm",
                "title": title,
                "installed": installed,
                "file": file_name(zip),
            }),
            Import::Running { file, progress, .. } => json!({
                "state": "running",
                "file": file,
                "done": progress.files_done,
                "total": progress.files_total,
                "entries": progress.entries,
            }),
        };
        json!({"rows": rows, "import": import})
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use nikomiru_dict::store::Counts;
    use nikomiru_dict::yomitan::Kinds;

    use super::*;

    fn record(title: &str, revision: &str) -> DictionaryRecord {
        DictionaryRecord {
            title: title.into(),
            revision: revision.into(),
            kinds: Kinds {
                terms: true,
                ..Kinds::default()
            },
            order: 0,
            enabled: true,
            language: Some("ja".into()),
            display_name: None,
            collapse: Collapse::Automatic,
            user_css: String::new(),
            counts: Counts {
                terms: 76,
                ..Counts::default()
            },
            imported_at: 0,
            size: 1024,
        }
    }

    #[derive(Default)]
    struct Fake {
        rows: Mutex<Vec<(String, DictionaryRecord)>>,
        checks: Mutex<Vec<(PathBuf, Check)>>,
        calls: Mutex<Vec<String>>,
    }

    impl DictionaryService for Fake {
        fn lookup(&self, _text: String, done: Done<Reply>) {
            done(Ok(Reply::default()));
        }
        fn list(&self) -> Result<Vec<(String, DictionaryRecord)>, String> {
            Ok(self.rows.lock().unwrap().clone())
        }
        fn check(&self, zip: &Path) -> Result<Check, String> {
            self.checks
                .lock()
                .unwrap()
                .iter()
                .find(|(p, _)| p == zip)
                .map(|(_, c)| c.clone())
                .ok_or_else(|| "not a dictionary".to_string())
        }
        fn import(
            &self,
            zip: PathBuf,
            replace: Option<String>,
            _cancel: Cancel,
            _progress: Box<dyn Fn(Progress) + Send>,
            _done: Done<String>,
        ) {
            self.calls
                .lock()
                .unwrap()
                .push(format!("import {} {replace:?}", zip.display()));
        }
        fn set_enabled(&self, id: &str, enabled: bool) -> Result<(), String> {
            for (i, r) in self.rows.lock().unwrap().iter_mut() {
                if i == id {
                    r.enabled = enabled;
                }
            }
            Ok(())
        }
        fn reorder(&self, ids: &[String]) -> Result<(), String> {
            let mut rows = self.rows.lock().unwrap();
            rows.sort_by_key(|(i, _)| ids.iter().position(|x| x == i));
            Ok(())
        }
        fn delete(&self, id: &str) -> Result<(), String> {
            self.rows.lock().unwrap().retain(|(i, _)| i != id);
            Ok(())
        }
        fn rename(&self, id: &str, name: Option<String>) -> Result<(), String> {
            for (i, r) in self.rows.lock().unwrap().iter_mut() {
                if i == id {
                    r.display_name.clone_from(&name);
                }
            }
            Ok(())
        }
        fn set_collapse(&self, _id: &str, _collapse: Collapse) -> Result<(), String> {
            Err("read-only".into())
        }
    }

    fn fake() -> Fake {
        let f = Fake::default();
        *f.rows.lock().unwrap() = vec![
            ("a".into(), record("JMdict", "1")),
            ("b".into(), record("Freq", "1")),
        ];
        *f.checks.lock().unwrap() = vec![
            ("/new.zip".into(), Check::New),
            ("/same.zip".into(), Check::SameRevision { id: "a".into() }),
            (
                "/newer.zip".into(),
                Check::Update {
                    id: "a".into(),
                    installed: "1".into(),
                },
            ),
        ];
        f
    }

    // T-DIC-016 (page)
    #[test]
    fn import_new_same_and_newer() {
        let f = fake();
        let mut page = DictionaryPage::default();
        page.refresh(&f);
        let e = page.choose(&f, "/same.zip".into());
        assert_eq!(
            e,
            [Effect::Message(
                "dict-same-revision",
                vec![("title", "JMdict".into())]
            )]
        );
        assert!(matches!(page.import, Import::Idle));

        let e = page.choose(&f, "/newer.zip".into());
        assert!(e.is_empty());
        assert_eq!(page.json()["import"]["state"], "confirm");
        assert_eq!(page.json()["import"]["installed"], "1");
        let e = page.confirm(true);
        assert!(
            matches!(&e[..], [Effect::Start { replace: Some(id), generation: 1, .. }] if id == "a")
        );
        assert_eq!(page.json()["import"]["state"], "running");
        // a second import waits for the first
        assert_eq!(
            page.choose(&f, "/new.zip".into()),
            [Effect::Message("dict-import-busy", Vec::new())]
        );

        page.progress(
            1,
            Progress {
                files_done: 2,
                files_total: 4,
                entries: 10,
            },
        );
        assert_eq!(page.json()["import"]["done"], 2);
        page.progress(
            7,
            Progress {
                files_done: 4,
                files_total: 4,
                entries: 99,
            },
        );
        assert_eq!(
            page.json()["import"]["done"],
            2,
            "another generation is ignored"
        );
        let e = page.finished(&f, 1, Ok("a".into()));
        assert_eq!(
            e,
            [Effect::Message(
                "dict-imported",
                vec![("title", "JMdict".into())]
            )]
        );
        assert_eq!(page.json()["import"]["state"], "idle");

        let e = page.choose(&f, "/broken.zip".into());
        assert_eq!(e, [Effect::Error("not a dictionary".into())]);
        let e = page.choose(&f, "/new.zip".into());
        assert!(
            matches!(&e[..], [Effect::Start { replace: None, generation: 2, cancel, .. }] if !cancel.is_cancelled())
        );
        page.cancel();
        let Import::Running { cancel, .. } = &page.import else {
            panic!()
        };
        assert!(cancel.is_cancelled());
        let e = page.finished(&f, 2, Err("import cancelled".into()));
        assert_eq!(e, [Effect::Error("new.zip: import cancelled".into())]);
    }

    // T-DIC-031, T-DIC-032, T-DIC-033
    #[test]
    fn order_enable_rename_delete() {
        let f = fake();
        let mut page = DictionaryPage::default();
        page.refresh(&f);
        page.move_by(&f, "b", -1);
        let ids: Vec<&str> = page.rows.iter().map(|(i, _)| i.as_str()).collect();
        assert_eq!(ids, ["b", "a"]);
        assert!(page.move_by(&f, "b", -1).is_empty(), "already first");
        page.set_enabled(&f, "a", false);
        assert_eq!(page.json()["rows"][1]["enabled"], false);
        page.rename(&f, "a", "  My JMdict ");
        assert_eq!(page.json()["rows"][1]["name"], "My JMdict");
        page.rename(&f, "a", "");
        assert_eq!(page.json()["rows"][1]["name"], "JMdict");
        assert_eq!(
            page.set_collapse(&f, "a", "collapsed"),
            [Effect::Error("read-only".into())]
        );
        page.delete(&f, "b");
        assert_eq!(page.rows.len(), 1);
        assert_eq!(page.json()["rows"][0]["types"], json!(["terms"]));
    }
}
