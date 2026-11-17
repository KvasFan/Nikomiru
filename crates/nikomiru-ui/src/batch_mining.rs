// SPDX-License-Identifier: GPL-3.0-only

//! Explicit frozen batch selections and opaque validated exporter tickets (BMG-001…BMG-004).
use crate::mining::{CardAction, FrameChoice, MineRequest, Mined};
use nikomiru_library::batches::{MAX_ITEMS, Status};
/// Combined frozen selection bytes, including already captured pictures.
pub const MAX_BYTES: usize = 8 << 20;
/// One captured original context; book identities survive text-only requests with no video link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// Frozen existing miner request; Click frames are never retained.
    pub request: MineRequest,
    /// Original source identity, including books.
    pub source_uid: Option<String>,
}
impl Candidate {
    /// Bound every captured input before retaining it in a queue.
    pub fn valid(&self) -> bool {
        let r = &self.request;
        !r.expression.is_empty()
            && r.expression.len() <= 4096
            && r.reading.len() <= 4096
            && r.sentence.len() <= 16384
            && r.query.len() <= 16384
            && r.matched.len() <= 4096
            && r.title.len() <= 1024
            && r.position_ms >= 0
            && r.position_ms <= i32::MAX as i64
            && r.composition == Default::default()
            && r.action == CardAction::Add
            && r.picture.as_ref().is_none_or(|p| p.len() <= 4 << 20)
            && r.offset.is_none_or(|offset| {
                r.sentence
                    .get(offset..)
                    .is_some_and(|s| s.starts_with(&r.matched))
            })
            && r.clip.is_none_or(|(start, end)| {
                start >= 0
                    && end > start
                    && end <= i32::MAX as i64
                    && ((!r.audio && r.picture.is_none() && r.frame == FrameChoice::None)
                        || end - start <= nikomiru_media::clip::MAX_AUDIO_MS)
            })
            && match r.frame {
                FrameChoice::Click => false,
                FrameChoice::At(ms) => ms >= 0 && ms <= i32::MAX as i64,
                FrameChoice::None => true,
            }
    }
    fn bytes(&self) -> usize {
        let r = &self.request;
        r.expression.len()
            + r.reading.len()
            + r.query.len()
            + r.matched.len()
            + r.sentence.len()
            + r.title.len()
            + r.picture.as_ref().map_or(0, Vec::len)
            + self.source_uid.as_ref().map_or(0, String::len)
    }
}
/// The exporter validates these hashes again before any extraction or mutation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ticket {
    /// Stable per-item operation UUID.
    pub operation: String,
    /// Exact request resolved to a dictionary entry during preview.
    pub request: MineRequest,
    /// Settings/tools/render-limit fingerprint.
    pub configuration: String,
    /// Dictionary fingerprint.
    pub dictionaries: String,
    /// Original file/series metadata fingerprint.
    pub source: String,
    /// Integrity of the complete frozen request.
    pub request_hash: String,
    /// Live note-type field names seen at preview time; empty for local/pending destinations.
    pub schema: Vec<String>,
}
/// Read-only text preview; media names are deliberately absent until extraction/export.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preview {
    /// Opaque validated request for the worker.
    pub ticket: Ticket,
    /// Configured destination kind.
    pub destination: String,
    /// Effective destination deck.
    pub deck: String,
    /// Effective note type.
    pub note_type: String,
    /// Effective tags, excluding the internal operation tag.
    pub tags: Vec<String>,
    /// Effective rendered text fields; planned media are indicated separately.
    pub fields: Vec<(String, String)>,
    /// Sentence audio is planned.
    pub audio: bool,
    /// A frame/captured picture is planned.
    pub image: bool,
}
/// An exporter's classified failure; uncertain outcomes never permit an automatic resend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    /// Translated reason.
    pub message: String,
    /// A mutation may already exist.
    pub uncertain: bool,
}
/// A per-item successful result, including a duplicate skipped before media extraction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The existing miner created/kept a card.
    Mined(Mined),
    /// Duplicate identity skipped.
    Duplicate,
}
/// One chosen word with a bounded list of frozen original contexts.
#[derive(Debug, Clone)]
pub struct Entry {
    /// Stable operation id, retained while contexts are edited before the initial Start.
    pub operation: String,
    /// Candidate contexts, never silently replaced by the active media.
    pub contexts: Vec<Candidate>,
    /// Explicit selected context index.
    pub selected: usize,
    /// Included in the next explicit preview/start.
    pub enabled: bool,
    /// Explicit sentence-audio choice, constrained by the captured source.
    pub audio: bool,
    /// Explicit image choice, constrained by the captured source.
    pub image: bool,
    /// Last complete effective preview.
    pub preview: Option<Preview>,
    /// Visible/durable outcome.
    pub status: Status,
}
impl Entry {
    /// Explicitly selected captured request.
    pub fn candidate(&self) -> Option<&Candidate> {
        self.contexts.get(self.selected)
    }
    /// Effective request after explicit media choices; the original capture is retained unchanged.
    pub fn request(&self) -> Option<MineRequest> {
        let mut r = self.candidate()?.request.clone();
        r.audio &= self.audio;
        if !self.image {
            r.frame = FrameChoice::None;
            r.picture = None;
        }
        Some(r)
    }
}
/// A bounded builder; service orchestration owns the running state separately.
#[derive(Debug, Default, Clone)]
pub struct Selection {
    /// Exact identities and context alternatives in chosen order.
    pub entries: Vec<Entry>,
}
impl Selection {
    /// Add one context, deduplicating exact identities and identical captures. All bounds are atomic.
    pub fn add(&mut self, candidate: Candidate) -> bool {
        if !candidate.valid() {
            return false;
        }
        let bytes = self
            .entries
            .iter()
            .flat_map(|e| &e.contexts)
            .map(Candidate::bytes)
            .sum::<usize>();
        if bytes.saturating_add(candidate.bytes()) > MAX_BYTES {
            return false;
        }
        let r = &candidate.request;
        if let Some(entry) = self.entries.iter_mut().find(|e| {
            e.contexts[0].request.expression == r.expression
                && e.contexts[0].request.reading == r.reading
        }) {
            if entry.contexts.contains(&candidate) {
                return true;
            }
            if entry.contexts.len() >= 5 {
                return false;
            }
            entry.contexts.push(candidate);
            entry.preview = None;
            return true;
        }
        if self.entries.len() >= MAX_ITEMS {
            return false;
        }
        let audio = candidate.request.audio && candidate.request.clip.is_some();
        let image = candidate.request.picture.is_some()
            || !matches!(candidate.request.frame, FrameChoice::None);
        self.entries.push(Entry {
            operation: uuid::Uuid::new_v4().to_string(),
            contexts: vec![candidate],
            selected: 0,
            enabled: true,
            audio,
            image,
            preview: None,
            status: Status::Planned,
        });
        true
    }
    /// Atomically extend a captured selection; no partial additions survive overflow.
    pub fn add_many(&mut self, candidates: Vec<Candidate>) -> bool {
        let mut staged = self.clone();
        for candidate in candidates {
            if !staged.add(candidate) {
                return false;
            }
        }
        *self = staged;
        true
    }
    /// Change a selected context explicitly; a prior preview cannot authorize the changed request.
    pub fn choose(&mut self, index: usize, context: usize) -> bool {
        let Some(entry) = self.entries.get_mut(index) else {
            return false;
        };
        if context >= entry.contexts.len() {
            return false;
        }
        entry.selected = context;
        let r = &entry.contexts[context].request;
        entry.audio = r.audio && r.clip.is_some();
        entry.image = r.picture.is_some() || !matches!(r.frame, FrameChoice::None);
        entry.preview = None;
        true
    }
    /// Freeze selected operation/request pairs for a whole-batch preview.
    pub fn requests(&self) -> Vec<(String, MineRequest)> {
        self.entries
            .iter()
            .filter(|e| e.enabled)
            .filter_map(|e| Some((e.operation.clone(), e.request()?)))
            .collect()
    }
}

/// One service request; the session dispatches at most one export at a time.
#[derive(Debug)]
pub enum ServiceJob {
    /// Read-only whole-batch preview.
    Preview(Vec<(String, MineRequest)>),
    /// Exactly one previously journalled operation.
    Mine(Box<Ticket>),
    /// Read-only exact operation query, including journal history after restart.
    Reconcile {
        /// Journal id.
        batch: String,
        /// Original persisted item.
        item: Box<nikomiru_library::batches::Item>,
    },
}
/// State-machine continuation after an ordered durable journal operation.
#[derive(Debug, Clone)]
pub enum After {
    /// History only.
    Loaded,
    /// Initial batch is durable.
    Started,
    /// Durable Sending marker, before dispatch.
    Marked(usize),
    /// An actual outcome is durable.
    Recorded(usize),
    /// Never-sent cancellation status only.
    StatusOnly,
    /// A reconciled existing note is recorded.
    Recovered {
        /// Stable operation id.
        operation: String,
        /// Existing note id.
        note: u64,
    },
    /// Explicit history cleanup.
    Deleted,
}
/// One ordered local write/read request.
#[derive(Debug, Clone)]
pub enum JournalJob {
    /// Load bounded history.
    Load,
    /// Write the whole selection before any card is handed to an exporter.
    Start(nikomiru_library::batches::Journal),
    /// Compare-and-set one outcome.
    Change {
        /// Journal id.
        batch: String,
        /// Stable operation id.
        operation: String,
        /// Expected current status.
        expected: Status,
        /// Desired allowed next status.
        next: Status,
        /// Continuation after commit.
        after: After,
    },
    /// Adopt a positively identified note, including original library provenance.
    Recover {
        /// Journal id.
        batch: String,
        /// Persisted original item.
        item: nikomiru_library::batches::Item,
        /// Uniquely identified note.
        note: u64,
        /// Time of local recovery.
        now_ms: i64,
    },
    /// Remove a resolved or never-sent journal explicitly.
    Delete(String),
}
/// Ordered journal reply, always identifying the continuation even on failure.
#[derive(Debug)]
pub struct JournalReply {
    /// Action completed.
    pub after: After,
    /// Complete bounded history or error.
    pub result: Result<Vec<nikomiru_library::batches::Journal>, String>,
}
/// Visible orchestration stage; no stage itself starts an export without an explicit Start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Stage {
    /// Builder is idle.
    #[default]
    Idle,
    /// Owned text is being segmented.
    Preparing,
    /// Text-only previews are being resolved.
    Previewing,
    /// Selection is previewed; Start is still required.
    Ready,
    /// Initial selection is being journalled.
    Starting,
    /// Sending marker is being committed.
    Marking(usize),
    /// Exactly one export is in flight.
    Sending(usize),
    /// Its actual outcome is being committed.
    Recording(usize),
    /// Explicit read-only reconciliation is in flight.
    Reconciling,
    /// Stopped on cancellation/failure/unknown.
    Stopped,
    /// Current finite run completed.
    Done,
}
impl Stage {
    /// Whether builder mutations must wait for the active stage.
    pub fn busy(self) -> bool {
        !matches!(self, Self::Idle | Self::Ready | Self::Stopped | Self::Done)
    }
}
/// Frozen media owner and selected identities for exact primary-track context preparation.
#[derive(Debug)]
pub struct ContextSeed {
    /// Captured original media request skeleton.
    pub request: MineRequest,
    /// Exact selected identities.
    pub wanted: std::collections::HashSet<(String, String)>,
    /// Padded original cue clips, by original cue index.
    pub clips: Vec<Option<(i64, i64)>>,
}
/// The complete bounded builder/run/history state.
#[derive(Debug, Default)]
pub struct State {
    /// Show the batch page above existing source pages.
    pub open: bool,
    /// Captured contexts and selected previews.
    pub selection: Selection,
    /// Current stage.
    pub stage: Stage,
    /// Last translated error.
    pub error: Option<String>,
    /// Explicit cancellation stops scheduling after the active card.
    pub stop_requested: bool,
    /// Durable journal for this builder's started selection.
    pub active: Option<String>,
    /// Physical selection rows in the current finite run.
    pub run: Vec<usize>,
    /// Current position in that finite run.
    pub cursor: usize,
    /// Loaded durable bounded history, including unresolved old operations.
    pub journals: Vec<nikomiru_library::batches::Journal>,
    /// Owned text preparation's cancellation token.
    pub cancel: Option<nikomiru_core::Cancel>,
    /// Frozen media owner during context preparation.
    pub(crate) seed: Option<ContextSeed>,
    /// Pending context request, dispatched through the existing dictionary worker.
    pub(crate) prepare: Option<(crate::sentence_bank::Request, nikomiru_core::Cancel)>,
    /// Deferred service work.
    pub(crate) service: Vec<ServiceJob>,
    /// Deferred serial library work.
    pub(crate) journal_jobs: Vec<JournalJob>,
}
#[cfg(test)]
mod tests {
    use super::*;
    fn candidate(expression: &str, sentence: &str) -> Candidate {
        Candidate {
            source_uid: Some("book/a".into()),
            request: MineRequest {
                composition: Default::default(),
                query: expression.into(),
                entry: 0,
                expression: expression.into(),
                reading: expression.into(),
                matched: expression.into(),
                sentence: sentence.into(),
                offset: None,
                video: Default::default(),
                audio_input: Default::default(),
                audio_stream: None,
                clip: None,
                position_ms: 0,
                uid: None,
                title: "Owned".into(),
                action: CardAction::Add,
                frame: FrameChoice::None,
                picture: None,
                audio: false,
            },
        }
    }
    // T-BMG-001: exact identity/context deduplication, stable operation ownership, explicit choice and bounded alternatives.
    #[test]
    fn batch_context_choices_are_frozen_bounded_and_share_one_operation_identity() {
        let mut s = Selection::default();
        let c = candidate("猫", "猫です。");
        assert!(s.add(c.clone()));
        let op = s.entries[0].operation.clone();
        assert!(s.add(c));
        assert_eq!(s.entries.len(), 1);
        assert_eq!(s.entries[0].contexts.len(), 1);
        for n in 1..5 {
            assert!(s.add(candidate("猫", &format!("猫{n}。"))));
        }
        assert!(!s.add(candidate("猫", "猫6。")));
        assert_eq!(s.entries[0].operation, op);
        assert!(s.choose(0, 4));
        assert_eq!(s.requests()[0].1.sentence, "猫4。");
        let mut bad = candidate("犬", "犬");
        bad.request.frame = FrameChoice::Click;
        assert!(!s.add(bad));
        for n in 1..MAX_ITEMS {
            assert!(s.add(candidate(&format!("word{n}"), "Owned")));
        }
        assert!(!s.add(candidate("overflow", "Owned")));
        assert_eq!(s.entries.len(), MAX_ITEMS);
    }
}
