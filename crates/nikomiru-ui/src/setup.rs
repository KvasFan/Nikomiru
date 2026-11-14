// SPDX-License-Identifier: GPL-3.0-only

//! The setup assistant (INS-001…INS-006, D-48): what the user ticked, what
//! the `setup` worker of the app tells as it downloads, and the state of
//! each item for the page. The dictionaries downloaded are imported one at
//! a time by the session, as a file chosen by hand is.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::PathBuf;

use nikomiru_online::setup::catalog::{Catalog, catalog};

/// What to install.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Plan {
    /// The dictionaries' ids in the catalogue.
    pub dictionaries: Vec<String>,
    /// The audio libraries' ids.
    pub audio: Vec<String>,
    /// Selected support tools, before dependency resolution.
    pub tools: Vec<String>,
    /// Current explicit executable paths.
    pub configured: ToolPaths,
}

/// Current tool settings captured for offline discovery/installation.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ToolPaths {
    /// Configured FFmpeg path.
    pub ffmpeg: String,
    /// Configured yt-dlp path.
    pub yt_dlp: String,
    /// Configured Deno path.
    pub deno: String,
}

/// Offline discovery result for one optional component.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolStatus {
    /// Catalog id, or ocr-model.
    pub id: String,
    /// Validated working executable; model uses its installed directory.
    pub path: Option<PathBuf>,
    /// Validated local location, for discovery diagnostics.
    pub detail: String,
    /// An install artifact exists for this target.
    pub supported: bool,
}

/// What the worker tells, for the item `id`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetupEvent {
    /// Offline tool inventory, requested when the assistant opens/refreshes.
    Inventory(Vec<ToolStatus>),
    /// Executable validation/extraction is underway.
    Checking {
        /// The item.
        id: String,
    },
    /// A complete executable bundle is validated and installed/reused.
    Tool {
        /// The item.
        id: String,
        /// Its validated executable.
        path: PathBuf,
    },
    /// The existing OCR model is installed/reused.
    Model {
        /// The model item.
        id: String,
    },
    /// `done` of `total` bytes are there.
    Progress {
        /// The item.
        id: String,
        /// Bytes there.
        done: u64,
        /// Its size.
        total: u64,
    },
    /// A dictionary is downloaded, to import.
    Dictionary {
        /// The item.
        id: String,
        /// Its file.
        path: PathBuf,
    },
    /// An audio library is downloaded and checked.
    Audio {
        /// The item.
        id: String,
        /// Its file.
        path: PathBuf,
    },
    /// It failed, translated; or it was stopped.
    Failed {
        /// The item.
        id: String,
        /// Why.
        message: String,
    },
    /// Nothing more is downloaded.
    Finished,
}

/// The downloads, away from the UI thread: the `setup` worker of the app.
pub trait SetupService: Send {
    /// Discover local components without making network requests.
    fn inspect(&self, _paths: ToolPaths, _events: Box<dyn Fn(SetupEvent) + Send + Sync>) {}
    /// Downloads what `plan` names, telling `events` as it goes.
    fn install(&self, plan: Plan, events: Box<dyn Fn(SetupEvent) + Send + Sync>);
    /// Stops what is under way; what is downloaded so far stays for later.
    fn stop(&self);
}

/// No downloads: every item fails with the reason.
#[derive(Debug)]
pub struct NoSetup(pub String);

impl SetupService for NoSetup {
    fn install(&self, plan: Plan, events: Box<dyn Fn(SetupEvent) + Send + Sync>) {
        for id in plan
            .dictionaries
            .into_iter()
            .chain(plan.audio)
            .chain(plan.tools)
        {
            events(SetupEvent::Failed {
                id,
                message: self.0.clone(),
            });
        }
        events(SetupEvent::Finished);
    }

    fn stop(&self) {}
}

/// Where an item is.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ItemState {
    /// Not asked.
    #[default]
    Idle,
    /// Asked, not started.
    Waiting,
    /// Being downloaded.
    Downloading {
        /// Bytes there.
        done: u64,
        /// Its size.
        total: u64,
    },
    /// Extracting and checking a downloaded executable.
    Checking,
    /// Waiting for its import, or being imported.
    Importing,
    /// Installed.
    Done,
    /// It failed, with the reason.
    Failed(String),
}

/// A job for the worker, taken by the UI layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetupJob {
    /// Download these.
    Install(Plan),
    /// Stop.
    Stop,
    /// Offline dependency detection.
    Inspect(ToolPaths),
}

/// The page's state in the session.
#[derive(Debug)]
pub struct SetupPage {
    /// Shown.
    pub open: bool,
    /// The catalogue.
    pub catalog: Catalog,
    /// Optional executable catalog.
    pub tools: Vec<nikomiru_online::setup::tools::Tool>,
    /// Worker-discovered local component state.
    pub inventory: BTreeMap<String, ToolStatus>,
    /// The items ticked, by id.
    pub ticked: BTreeSet<String>,
    /// Where each item is.
    pub items: BTreeMap<String, ItemState>,
    /// Downloads are under way.
    pub running: bool,
    /// Dictionaries downloaded, waiting for their import.
    pub imports: VecDeque<(String, PathBuf)>,
    /// The one being imported: its id, its file, its import's generation.
    pub importing: Option<(String, PathBuf, u64)>,
}

impl Default for SetupPage {
    fn default() -> Self {
        let catalog = catalog();
        let ticked = catalog
            .dictionaries
            .iter()
            .filter(|d| d.ticked)
            .map(|d| d.id.clone())
            .chain(
                catalog
                    .audio
                    .iter()
                    .filter(|a| a.ticked)
                    .map(|a| a.id.clone()),
            )
            .collect();
        Self {
            open: false,
            catalog,
            tools: nikomiru_online::setup::tools::catalog(),
            inventory: BTreeMap::new(),
            ticked,
            items: BTreeMap::new(),
            running: false,
            imports: VecDeque::new(),
            importing: None,
        }
    }
}

impl SetupPage {
    /// The ids of the catalogue's dictionaries installed already, from the
    /// titles of the dictionaries there (INS-002).
    pub fn installed<'a>(&self, titles: impl Iterator<Item = &'a str> + Clone) -> BTreeSet<String> {
        self.catalog
            .dictionaries
            .iter()
            .filter(|d| titles.clone().any(|t| t.starts_with(&d.title_prefix)))
            .map(|d| d.id.clone())
            .collect()
    }

    /// The plan of what is ticked, without what is installed or done.
    pub fn plan(&self, installed: &BTreeSet<String>) -> Plan {
        let wanted = |id: &String| {
            self.ticked.contains(id)
                && !installed.contains(id)
                && self.items.get(id) != Some(&ItemState::Done)
        };
        let requested: Vec<_> = self
            .tools
            .iter()
            .filter(|tool| {
                self.ticked.contains(&tool.id) && self.items.get(&tool.id) != Some(&ItemState::Done)
            })
            .map(|tool| tool.id.clone())
            .collect();
        let mut tools =
            nikomiru_online::setup::tools::resolve(&self.tools, &requested).unwrap_or(requested);
        if self.ticked.contains("ocr-model")
            && self.items.get("ocr-model") != Some(&ItemState::Done)
        {
            tools.push("ocr-model".into());
        }
        Plan {
            tools,
            configured: ToolPaths::default(),
            dictionaries: self
                .catalog
                .dictionaries
                .iter()
                .map(|d| &d.id)
                .filter(|id| wanted(id))
                .cloned()
                .collect(),
            audio: self
                .catalog
                .audio
                .iter()
                .map(|a| &a.id)
                .filter(|id| wanted(id))
                .cloned()
                .collect(),
        }
    }

    /// An event of the worker.
    pub fn event(&mut self, event: &SetupEvent) {
        match event {
            SetupEvent::Inventory(rows) => {
                for row in rows {
                    if row.path.is_none() && self.items.get(&row.id) == Some(&ItemState::Done) {
                        self.items.remove(&row.id);
                    }
                }
                self.inventory = rows.iter().map(|r| (r.id.clone(), r.clone())).collect();
            }
            SetupEvent::Checking { id } => {
                self.items.insert(id.clone(), ItemState::Checking);
            }
            SetupEvent::Tool { id, .. } | SetupEvent::Model { id } => {
                self.items.insert(id.clone(), ItemState::Done);
            }
            SetupEvent::Progress { id, done, total } => {
                self.items.insert(
                    id.clone(),
                    ItemState::Downloading {
                        done: *done,
                        total: *total,
                    },
                );
            }
            SetupEvent::Dictionary { id, path } => {
                self.items.insert(id.clone(), ItemState::Importing);
                self.imports.push_back((id.clone(), path.clone()));
            }
            SetupEvent::Audio { id, .. } => {
                self.items.insert(id.clone(), ItemState::Done);
            }
            SetupEvent::Failed { id, message } => {
                self.items
                    .insert(id.clone(), ItemState::Failed(message.clone()));
            }
            SetupEvent::Finished => self.running = false,
        }
    }

    /// Something is under way: a download or an import.
    pub fn busy(&self) -> bool {
        self.running
            || self.importing.is_some()
            || !self.imports.is_empty()
            || self.items.values().any(|s| *s == ItemState::Checking)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-INS-002: a dictionary installed is left out; the events move the rows
    #[test]
    fn the_plan_and_the_items() {
        let mut page = SetupPage::default();
        let installed = page.installed(["Jitendex.org [2026-10-03]", "Other"].into_iter());
        assert_eq!(installed, BTreeSet::from(["jitendex".to_string()]));
        let plan = page.plan(&installed);
        assert_eq!(plan.dictionaries, ["kanjidic", "kanjium", "jpdb"]);
        assert_eq!(plan.audio, ["fushi-ja"]);
        page.running = true;
        page.event(&SetupEvent::Progress {
            id: "kanjidic".into(),
            done: 5,
            total: 10,
        });
        page.event(&SetupEvent::Dictionary {
            id: "kanjidic".into(),
            path: "/c/kanjidic.zip".into(),
        });
        page.event(&SetupEvent::Failed {
            id: "fushi-ja".into(),
            message: "no".into(),
        });
        page.event(&SetupEvent::Finished);
        assert_eq!(page.items["kanjidic"], ItemState::Importing);
        assert_eq!(page.imports.len(), 1);
        assert!(page.busy() && !page.running);
        page.items.insert("kanjium".into(), ItemState::Done);
        assert!(
            !page
                .plan(&installed)
                .dictionaries
                .contains(&"kanjium".to_string())
        );
    }
    // T-INS-007: selecting a root resolves required tools; inventory never starts a download.
    #[test]
    fn selected_tools_resolve_dependencies_and_can_reuse_inventory() {
        let mut page = SetupPage::default();
        page.ticked.clear();
        assert!(page.plan(&BTreeSet::new()).tools.is_empty());
        page.ticked.insert("yt-dlp".into());
        page.event(&SetupEvent::Inventory(vec![ToolStatus {
            id: "ffmpeg".into(),
            path: Some("/system/ffmpeg".into()),
            detail: String::new(),
            supported: true,
        }]));
        // Keep available tools in the plan: install adopts and saves their validated paths.
        assert_eq!(
            page.plan(&BTreeSet::from(["ffmpeg".into()])).tools,
            ["ffmpeg", "deno", "yt-dlp"]
        );
        page.ticked.remove("yt-dlp");
        assert!(page.plan(&BTreeSet::new()).tools.is_empty());
        page.ticked.insert("ocr-model".into());
        assert_eq!(page.plan(&BTreeSet::new()).tools, ["ocr-model"]);
        page.event(&SetupEvent::Model {
            id: "ocr-model".into(),
        });
        assert!(page.plan(&BTreeSet::new()).tools.is_empty());
        page.event(&SetupEvent::Inventory(vec![ToolStatus {
            id: "ocr-model".into(),
            path: None,
            detail: String::new(),
            supported: true,
        }]));
        assert_eq!(page.plan(&BTreeSet::new()).tools, ["ocr-model"]);
    }
}
