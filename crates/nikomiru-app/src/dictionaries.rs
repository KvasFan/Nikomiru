// SPDX-License-Identifier: GPL-3.0-only

//! The dictionary service of the session: lookups run on a worker thread,
//! with the dictionaries of the registry (DIC-021), and come back as popup
//! content (POP-007).

use std::sync::Arc;
use std::sync::mpsc;

use nikomiru_dict::css::Sheet;
use nikomiru_dict::lookup::Options;
use nikomiru_library::{Collapse as RecordCollapse, DictionaryError, DictionaryManager};
use nikomiru_ui::i18n::Translator;
use nikomiru_ui::popup::Reply;
use nikomiru_ui::popup_view::{self, Collapse, DictView};
use nikomiru_ui::session::LookupService;

type Done = Box<dyn FnOnce(Result<Reply, String>) + Send>;

/// Runs lookups on its own thread, in order.
#[derive(Debug)]
pub struct Lookups {
    jobs: mpsc::Sender<(String, Done)>,
}

impl Lookups {
    /// Starts the worker.
    pub fn start(manager: Arc<DictionaryManager>, translator: Translator) -> Self {
        let (jobs, rx) = mpsc::channel::<(String, Done)>();
        let spawned = std::thread::Builder::new()
            .name("nikomiru-lookup".into())
            .spawn(move || {
                for (text, done) in rx {
                    let started = std::time::Instant::now();
                    let result = render(&manager, &text).map_err(|e| translator.error(&e));
                    tracing::debug!(query = %text, elapsed = ?started.elapsed(), "lookup");
                    done(result);
                }
            });
        if let Err(e) = spawned {
            tracing::error!("cannot start the lookup thread: {e}");
        }
        Self { jobs }
    }
}

impl LookupService for Lookups {
    fn lookup(&self, text: String, done: Done) {
        if let Err(mpsc::SendError((_, done))) = self.jobs.send((text, done)) {
            done(Err("the dictionaries are not available".into()));
        }
    }
}

fn is_han(c: char) -> bool {
    matches!(c as u32, 0x4E00..=0x9FFF | 0x3400..=0x4DBF | 0x20000..=0x2A6DF | 0xF900..=0xFAFF)
}

fn render(manager: &DictionaryManager, text: &str) -> Result<Reply, DictionaryError> {
    let lookup = manager.lookup()?;
    let entries = lookup.lookup(text, Options::default())?;
    // a single kanji also shows its kanji entries (LKP-037)
    let mut chars = text.chars();
    let kanji = match (chars.next(), chars.next()) {
        (Some(c), None) if is_han(c) => lookup.kanji(text)?,
        _ => Vec::new(),
    };
    let records = manager.list()?;
    let dicts: Vec<DictView> = lookup
        .dictionaries()
        .iter()
        .map(|d| {
            let record = records.iter().find(|(id, _)| *id == d.id).map(|(_, r)| r);
            let styles = d.store.info().styles.clone();
            let user_css = record.map(|r| r.user_css.as_str()).unwrap_or_default();
            DictView {
                name: d.name.clone(),
                collapse: match record.map(|r| r.collapse) {
                    Some(RecordCollapse::Expanded) => Collapse::Expanded,
                    Some(RecordCollapse::Collapsed) => Collapse::Collapsed,
                    _ => Collapse::Automatic,
                },
                sheet: Sheet::parse(&format!("{styles}\n{user_css}")),
            }
        })
        .collect();
    let tags = |i: usize, name: &str| {
        lookup
            .dictionaries()
            .get(i)
            .and_then(|d| d.store.tag(name).ok().flatten())
    };
    let media = |i: usize, path: &str| {
        lookup
            .dictionaries()
            .get(i)
            .and_then(|d| d.store.media(path).ok().flatten())
    };
    let content = popup_view::content(&entries, &kanji, &dicts, &tags, &media);
    Ok(Reply {
        content: content.to_string(),
        best_length: nikomiru_dict::lookup::best_length(&entries),
        empty: entries.is_empty() && kanji.is_empty(),
    })
}
