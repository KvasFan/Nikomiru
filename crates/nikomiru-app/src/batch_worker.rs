// SPDX-License-Identifier: GPL-3.0-only

//! Effective bounded previews and exact-operation batch export/reconciliation.
use super::*;
use nikomiru_library::DictionaryError;
use nikomiru_ui::batch_mining::{Candidate, Failure, Outcome, Preview, Ticket};
use serde::Serialize;
use sha2::{Digest, Sha256};
#[derive(Debug)]
struct HashWriter(Sha256);
type TextPreview = (Vec<(String, String)>, Vec<String>);
impl std::io::Write for HashWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.update(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn hash(value: &impl Serialize) -> Option<String> {
    let mut w = HashWriter(Sha256::new());
    serde_json::to_writer(&mut w, value).ok()?;
    Some(w.0.finalize().iter().map(|b| format!("{b:02x}")).collect())
}
pub(super) fn operation_tag(id: &str) -> String {
    format!("nikomiru_batch::{id}")
}
fn file_stamp(path: &Path) -> Option<(PathBuf, u64, String)> {
    if path.as_os_str().is_empty() {
        return Some((PathBuf::new(), 0, String::new()));
    }
    if nikomiru_ui::is_web_address(&path.to_string_lossy()) {
        return Some((path.to_owned(), 0, "url".into()));
    }
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() {
        return None;
    }
    let time = match meta.modified().ok()?.duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => format!("+{}", d.as_nanos()),
        Err(e) => format!("-{}", e.duration().as_nanos()),
    };
    Some((path.to_owned(), meta.len(), time))
}
impl Worker {
    fn batch_error(&self, key: &str) -> String {
        self.parts.translator.tr(key, &[])
    }
    fn configuration_hash(&self) -> Option<String> {
        hash(&(
            &self.parts.config,
            &self.parts.ffmpeg,
            self.parts.max_results.load(Ordering::Relaxed),
        ))
    }
    fn dictionary_hash(&self) -> Option<String> {
        hash(&self.parts.dictionaries.as_ref()?.analysis_identity().ok()?)
    }
    fn source_hash(&self, r: &MineRequest) -> Option<String> {
        let video = if !r.audio && matches!(r.frame, FrameChoice::None) && r.picture.is_none() {
            (PathBuf::new(), 0, String::new())
        } else {
            file_stamp(&r.video)?
        };
        let audio = if r.audio {
            file_stamp(&r.audio_input)?
        } else {
            (PathBuf::new(), 0, String::new())
        };
        let series = r.uid.as_deref().and_then(|uid| {
            self.parts
                .library
                .as_ref()?
                .series_title_of_video(uid)
                .ok()
                .flatten()
        });
        hash(&(video, audio, series))
    }
    fn preview_fields(&mut self, r: &MineRequest, operation: &str) -> Result<TextPreview, String> {
        let dict = self.dictionary_fields(r).map_err(|e| e.message)?;
        if self.parts.config.to_review() {
            return Ok((
                vec![
                    ("Expression".into(), r.expression.clone()),
                    ("Reading".into(), r.reading.clone()),
                    ("Sentence".into(), r.sentence.clone()),
                    (
                        "Definition".into(),
                        nikomiru_anki::html::text(&dict.glossary),
                    ),
                ],
                Vec::new(),
            ));
        }
        let fields = if self.parts.config.later() {
            if let Some(mapping) = self
                .parts
                .config
                .field_mapping
                .get(&self.parts.config.note_type)
                .filter(|m| !m.is_empty())
            {
                mapping.keys().cloned().collect()
            } else if lapis::is_lapis(&self.parts.config.note_type, &[]) {
                lapis::FIELDS.iter().map(|s| s.to_string()).collect()
            } else {
                return Err(self.batch_error("batch-schema-needed"));
            }
        } else {
            self.note_fields().map_err(|e| self.message(&e))?
        };
        if fields.is_empty() || fields.len() > 128 {
            return Err(self.batch_error("batch-invalid"));
        }
        let (start, end) = r.clip.unwrap_or((r.position_ms, r.position_ms));
        let sentence = compose::sentence(&r.sentence, r.offset, &r.matched);
        let source = r.uid.as_ref().map(|uid| SourceLink {
            uid: uid.clone(),
            episode: 0,
            start_ms: start.max(0) as u32,
            end_ms: end.max(0) as u32,
            source_id: operation.into(),
        });
        let scene = SceneFields {
            sentence: sentence.clone(),
            cue_sentence: sentence,
            sentence_audio: String::new(),
            card_image: String::new(),
            audio: String::new(),
            document_title: nikomiru_anki::html::escape(&r.title),
            source_link: source.map_or_else(
                || nikomiru_anki::html::escape(&r.title),
                |s| s.to_html(&r.title),
            ),
            clip_timestamp: if r.clip.is_some() {
                compose::clip_timestamp(start, end)
            } else {
                String::new()
            },
        };
        let values = compose::values(&dict, &scene);
        let mut rendered = compose::fields(
            &fields,
            &self.mapping(&fields),
            &|name| values.get(name).cloned().unwrap_or_default(),
            false,
        )
        .map_err(|e| self.fields_failure(&e).message)?;
        if lapis::is_lapis(&self.parts.config.note_type, &fields) {
            rendered = compose::lapis_safe(rendered);
        }
        // A context/media-dependent first field cannot safely deduplicate a new batch by word identity.
        if !self.parts.config.later() {
            let identity = self
                .first_field(&r.expression, &r.reading)
                .map_err(|e| self.message(&e))?
                .ok_or_else(|| self.batch_error("batch-identity-field"))?;
            if identity.1.is_empty()
                || rendered
                    .iter()
                    .find(|(name, _)| *name == identity.0)
                    .is_none_or(|(_, value)| *value != identity.1)
            {
                return Err(self.batch_error("batch-identity-field"));
            }
        }
        let series = r.uid.as_deref().and_then(|uid| {
            self.parts
                .library
                .as_ref()?
                .series_title_of_video(uid)
                .ok()
                .flatten()
        });
        Ok((
            rendered,
            compose::tags(&self.parts.config.tags, series.as_deref()),
        ))
    }
    pub(super) fn batch_check_guard(&self) -> Result<(), MineFailure> {
        if let Some(ticket) = &self.batch_guard
            && (self.configuration_hash().as_deref() != Some(&ticket.configuration)
                || self.dictionary_hash().as_deref() != Some(&ticket.dictionaries)
                || self.source_hash(&ticket.request).as_deref() != Some(&ticket.source))
        {
            return Err(self.fail_with("batch-stale-preview", &[]));
        }
        Ok(())
    }
    pub(super) fn batch_preview(
        &mut self,
        requests: Vec<(String, MineRequest)>,
    ) -> Result<Vec<Preview>, String> {
        let invalid = self.batch_error("batch-invalid");
        if requests.is_empty() || requests.len() > 20 {
            return Err(invalid.clone());
        }
        let configuration = self.configuration_hash().ok_or_else(|| invalid.clone())?;
        if !self.parts.config.to_review() && !self.parts.config.later() {
            self.note_fields = None;
        }
        let dictionaries = self.dictionary_hash().ok_or_else(|| invalid.clone())?;
        let mut operations = std::collections::HashSet::new();
        let mut identities = std::collections::HashSet::new();
        let mut out = Vec::new();
        let mut bytes = 0usize;
        let journals = self
            .parts
            .library
            .as_ref()
            .map(|l| l.batch_journals())
            .transpose()
            .map_err(|e| e.to_string())?
            .unwrap_or_default();
        for (operation, mut r) in requests {
            if journals
                .iter()
                .flat_map(|j| &j.items)
                .any(|item| item.operation == operation && item.status.unresolved())
            {
                return Err(self.batch_error("batch-unknown"));
            }
            if !nikomiru_library::batches::valid_id(&operation)
                || !operations.insert(operation.clone())
                || !identities.insert((r.expression.clone(), r.reading.clone()))
                || !(Candidate {
                    source_uid: r.uid.clone(),
                    request: r.clone(),
                })
                .valid()
            {
                return Err(self.batch_error("batch-invalid"));
            }
            let manager = self
                .parts
                .dictionaries
                .as_ref()
                .ok_or_else(|| self.batch_error("batch-invalid"))?;
            let lookup = manager.lookup().map_err(|e| self.message(&e))?;
            let entries = lookup
                .lookup(
                    &r.query,
                    Options {
                        max_results: self.parts.max_results.load(Ordering::Relaxed),
                    },
                )
                .map_err(|e| self.message(&DictionaryError::from(e)))?;
            r.entry = entries
                .iter()
                .position(|e| e.expression == r.expression && e.reading == r.reading)
                .ok_or_else(|| self.batch_error("error-mining-dictionaries-changed"))?;
            let source = self
                .source_hash(&r)
                .ok_or_else(|| self.batch_error("batch-source-changed"))?;
            let (fields, tags) = self.preview_fields(&r, &operation)?;
            if fields.iter().map(|(k, v)| k.len() + v.len()).sum::<usize>() > 256 << 10 {
                return Err(self.batch_error("batch-invalid"));
            }
            bytes =
                bytes.saturating_add(fields.iter().map(|(k, v)| k.len() + v.len()).sum::<usize>());
            if bytes > 5 << 20
                || fields
                    .iter()
                    .any(|(k, v)| k.len() > 256 || v.len() > 64 << 10)
            {
                return Err(self.batch_error("batch-invalid"));
            }
            let request_hash = hash(&r).ok_or_else(|| self.batch_error("batch-invalid"))?;
            let preview = Preview {
                audio: r.audio && r.clip.is_some(),
                image: r.picture.is_some() || !matches!(r.frame, FrameChoice::None),
                deck: self.parts.config.deck.clone(),
                note_type: self.parts.config.note_type.clone(),
                destination: self.parts.config.destination.clone(),
                tags,
                fields,
                ticket: Ticket {
                    operation,
                    request: r,
                    configuration: configuration.clone(),
                    dictionaries: dictionaries.clone(),
                    source,
                    request_hash,
                    schema: if self.parts.config.to_review() || self.parts.config.later() {
                        Vec::new()
                    } else {
                        self.note_fields.clone().unwrap_or_default()
                    },
                },
            };
            out.push(preview);
        }
        if self.configuration_hash().as_deref() != Some(&configuration)
            || self.dictionary_hash().as_deref() != Some(&dictionaries)
        {
            return Err(self.batch_error("batch-stale-preview"));
        }
        Ok(out)
    }
    pub(super) fn batch_reconcile(&mut self, operation: &str) -> Result<Option<u64>, String> {
        if !nikomiru_library::batches::valid_id(operation) {
            return Err(self.batch_error("batch-invalid"));
        }
        let tag = operation_tag(operation);
        let client = self.client().map_err(|e| self.message(&e))?;
        let ids = client
            .find_notes(&format!("tag:{tag}"))
            .map_err(|e| self.message(&e))?;
        if ids.is_empty() {
            return Ok(None);
        }
        if ids.len() != 1 {
            return Err(self.batch_error("batch-reconcile-ambiguous"));
        }
        let notes = client.notes_info(&ids).map_err(|e| self.message(&e))?;
        if notes
            .first()
            .and_then(Option::as_ref)
            .is_none_or(|n| n.id != ids[0] || !n.tags.iter().any(|t| t.eq_ignore_ascii_case(&tag)))
        {
            return Err(self.batch_error("batch-reconcile-ambiguous"));
        }
        self.batch_unresolved.remove(operation);
        Ok(Some(ids[0]))
    }
    pub(super) fn batch_mine(&mut self, ticket: Ticket) -> Result<Outcome, Failure> {
        let already_unknown = self.batch_unresolved.contains(&ticket.operation);
        let fail = |message| Failure {
            message,
            uncertain: already_unknown,
        };
        if self.configuration_hash().as_deref() != Some(&ticket.configuration)
            || self.dictionary_hash().as_deref() != Some(&ticket.dictionaries)
            || hash(&ticket.request).as_deref() != Some(&ticket.request_hash)
        {
            return Err(fail(self.batch_error("batch-stale-preview")));
        }
        if self.source_hash(&ticket.request).as_deref() != Some(&ticket.source) {
            return Err(fail(self.batch_error("batch-source-changed")));
        }
        if !nikomiru_library::batches::valid_id(&ticket.operation) {
            return Err(fail(self.batch_error("batch-invalid")));
        }
        let r = &ticket.request;
        if !self.parts.config.to_review() && !self.parts.config.later() {
            let fields = self
                .client()
                .map_err(|e| fail(self.message(&e)))?
                .model_field_names(&self.parts.config.note_type)
                .map_err(|e| fail(self.message(&e)))?;
            if fields != ticket.schema {
                self.note_fields = None;
                return Err(fail(self.batch_error("batch-stale-preview")));
            }
        }
        if self.parts.config.to_review() {
            if self
                .parts
                .library
                .as_ref()
                .ok_or_else(|| fail(self.batch_error("batch-invalid")))?
                .review_has(&[(r.expression.clone(), r.reading.clone())])
                .map_err(|e| fail(e.to_string()))?
                .first()
                == Some(&true)
            {
                return Ok(Outcome::Duplicate);
            }
        } else if self.parts.config.later() {
            if later::list(&self.parts.later_dir)
                .iter()
                .any(|c| c.expression == r.expression && c.reading == r.reading)
            {
                return Ok(Outcome::Duplicate);
            }
        } else {
            if let Some(id) = self.batch_reconcile(&ticket.operation).map_err(fail)? {
                let times = r.clip.unwrap_or((r.position_ms, r.position_ms));
                self.record(id, &Record::of(r, &ticket.operation, times));
                return Ok(Outcome::Mined(Mined {
                    note_id: id,
                    deck: self.parts.config.deck.clone(),
                    overwritten: false,
                    kept: None,
                }));
            }
            if self.batch_unresolved.contains(&ticket.operation) {
                return Err(Failure {
                    message: self.batch_error("batch-unknown"),
                    uncertain: true,
                });
            }
            let first = self
                .first_field(&r.expression, &r.reading)
                .map_err(|e| fail(self.message(&e)))?
                .ok_or_else(|| fail(self.batch_error("batch-identity-field")))?;
            let note = Note {
                deck: self.parts.config.deck.clone(),
                model: self.parts.config.note_type.clone(),
                fields: vec![first],
                tags: Vec::new(),
                scope: self.scope(),
            };
            let duplicate = self
                .client()
                .map_err(|e| fail(self.message(&e)))?
                .duplicates(&[note])
                .map_err(|e| fail(self.message(&e)))?;
            if duplicate.first() == Some(&true) {
                return Ok(Outcome::Duplicate);
            }
        }
        if self.batch_unresolved.len() >= 2000 {
            return Err(fail(self.batch_error("batch-invalid")));
        }
        self.batch_unknown = false;
        self.batch_guard = Some(ticket.clone());
        self.batch_operation = Some(ticket.operation.clone());
        let result = self.mine(r, None);
        self.batch_operation = None;
        self.batch_guard = None;
        if result.is_err() && self.batch_unknown {
            self.batch_unresolved.insert(ticket.operation.clone());
        }
        match result {
            Ok(mined) => Ok(Outcome::Mined(mined)),
            Err(e) if e.duplicate => Ok(Outcome::Duplicate),
            Err(e) => Err(Failure {
                message: e.message,
                uncertain: self.batch_unknown,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nikomiru_anki::fake::{Behaviour, FakeAnki};
    use std::io::Write;
    fn worker(root: &Path, fake: &FakeAnki) -> Worker {
        let library = Arc::new(Library::open(&root.join("library.redb")).unwrap());
        let clock: Arc<dyn Clock> = Arc::new(nikomiru_core::SystemClock::new());
        let manager = Arc::new(
            DictionaryManager::open(
                Arc::clone(&library),
                root.join("dicts"),
                Arc::clone(&clock),
                "ja",
            )
            .unwrap(),
        );
        let zip_path = root.join("mini.zip");
        let mut archive = zip::ZipWriter::new(std::fs::File::create(&zip_path).unwrap());
        let fixtures =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/dicts/mini-ja");
        for name in [
            "index.json",
            "term_bank_1.json",
            "term_bank_2.json",
            "tag_bank_1.json",
            "styles.css",
            "img/a.png",
        ] {
            archive
                .start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            archive
                .write_all(&std::fs::read(fixtures.join(name)).unwrap())
                .unwrap();
        }
        archive.finish().unwrap();
        manager
            .import(&zip_path, None, &Cancel::new(), |_| {})
            .unwrap();
        let config = AnkiConfig {
            url: fake.url(),
            ..Default::default()
        };
        lapis::create(&Client::new(&fake.url(), "").unwrap()).unwrap();
        Worker::new(Parts {
            config,
            ffmpeg: "missing-ffmpeg-for-preview-test".into(),
            translator: Translator::new("en"),
            dictionaries: Some(manager),
            max_results: Arc::new(AtomicUsize::new(32)),
            cache_dir: root.join("media"),
            library: Some(library),
            review_dir: root.join("review"),
            later_dir: root.join("later"),
            clock,
        })
    }
    fn request() -> MineRequest {
        MineRequest {
            composition: Default::default(),
            query: "猫です".into(),
            entry: 0,
            expression: "猫".into(),
            reading: "ねこ".into(),
            matched: "猫".into(),
            sentence: "猫です。".into(),
            offset: Some(0),
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
        }
    }
    fn no_process_tools(w: &mut Worker) {
        // A text-only export may not invoke either tool. Supplying nonexistent paths makes accidental use fail.
        w.tools = Some(Tools {
            ffmpeg: "never-invoke-ffmpeg".into(),
            ffprobe: "never-invoke-ffprobe".into(),
            origin: nikomiru_media::tools::Origin::Setting,
            version: String::new(),
            encoders: Default::default(),
        });
    }
    // T-BMG-002, T-BMG-004: real dictionary/schema previews never extract media, and lost commit replies reconcile by exact operation without another add.
    #[test]
    fn batch_preview_and_unknown_commit_recovery_do_not_duplicate_notes() {
        let fake = FakeAnki::start().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let mut w = worker(dir.path(), &fake);
        let op = "00000000-0000-4000-8000-000000000001";
        let preview = w
            .batch_preview(vec![(op.into(), request())])
            .unwrap()
            .remove(0);
        assert!(
            preview
                .fields
                .iter()
                .any(|(k, v)| k == "Sentence" && v.contains("猫"))
        );
        assert!(!preview.audio && !preview.image);
        assert!(w.tools.is_none());
        assert!(!w.parts.cache_dir.exists());
        assert!(fake.collection().notes.is_empty());
        no_process_tools(&mut w);
        fake.on("addNote", Behaviour::ApplyThenClose);
        let failure = w.batch_mine(preview.ticket.clone()).unwrap_err();
        assert!(failure.uncertain);
        assert_eq!(fake.collection().notes.len(), 1);
        assert!(fake.collection().notes[0].tags.contains(&operation_tag(op)));
        let note = w.batch_reconcile(op).unwrap().unwrap();
        assert!(
            matches!(w.batch_mine(preview.ticket).unwrap(),Outcome::Mined(Mined { note_id,.. }) if note_id==note)
        );
        assert_eq!(
            fake.requests()
                .iter()
                .filter(|r| r["action"] == "addNote")
                .count(),
            1
        );
    }
    // T-BMG-003, T-BMG-004: prepared batch cards retain operation identities and unknown-outcome protection in the ordinary later exporter.
    #[test]
    fn pending_batch_operation_tags_reconcile_before_any_later_resend() {
        let fake = FakeAnki::start().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let mut w = worker(dir.path(), &fake);
        no_process_tools(&mut w);
        let mut config = w.parts.config.clone();
        config.destination = "anki-later".into();
        w.configure(config);
        let op = "00000000-0000-4000-8000-000000000003";
        let preview = w
            .batch_preview(vec![(op.into(), request())])
            .unwrap()
            .remove(0);
        assert!(matches!(
            w.batch_mine(preview.ticket).unwrap(),
            Outcome::Mined(Mined { kept: Some(1), .. })
        ));
        let pending = later::list(&w.parts.later_dir);
        assert_eq!(pending.len(), 1);
        assert_eq!(
            later::load(&w.parts.later_dir, &pending[0].id)
                .unwrap()
                .0
                .record
                .batch_operation
                .as_deref(),
            Some(op)
        );
        fake.on("addNote", Behaviour::ApplyThenClose);
        let first = w.send_later();
        assert_eq!(first.failed, 1);
        assert_eq!(fake.collection().notes.len(), 1);
        assert!(later::uncertain(&w.parts.later_dir, &pending[0].id).unwrap());
        let recovered = w.send_later();
        assert_eq!(recovered.sent, 1);
        assert!(recovered.cards.is_empty());
        assert_eq!(
            fake.requests()
                .iter()
                .filter(|r| r["action"] == "addNote")
                .count(),
            1
        );
    }
    // T-BMG-002, T-BMG-004: unresolved zero-match operations never resend, and mutable settings or altered requests invalidate preview tickets.
    #[test]
    fn zero_match_unknown_operations_and_stale_previews_cannot_add() {
        let fake = FakeAnki::start().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let mut w = worker(dir.path(), &fake);
        no_process_tools(&mut w);
        let op = "00000000-0000-4000-8000-000000000002";
        let preview = w
            .batch_preview(vec![(op.into(), request())])
            .unwrap()
            .remove(0);
        let mut altered = preview.ticket.clone();
        altered.request.sentence = "犬".into();
        assert!(!w.batch_mine(altered).unwrap_err().uncertain);
        assert!(fake.collection().notes.is_empty());
        fake.on("addNote", Behaviour::Close);
        assert!(w.batch_mine(preview.ticket.clone()).unwrap_err().uncertain);
        assert_eq!(w.batch_reconcile(op).unwrap(), None);
        assert!(w.batch_mine(preview.ticket.clone()).unwrap_err().uncertain);
        assert_eq!(
            fake.requests()
                .iter()
                .filter(|r| r["action"] == "addNote")
                .count(),
            1
        );
        let mut config = w.parts.config.clone();
        config.tags.push("changed".into());
        w.configure(config);
        assert!(w.batch_mine(preview.ticket).is_err());
        assert!(fake.collection().notes.is_empty());
    }
    // T-MCP-002, T-MCP-003: invalid fields/senses never reach extraction or mutation; exact selection and literals survive pending export.
    #[test]
    fn composer_validates_before_media_and_preserves_literals_through_pending_export() {
        let fake = FakeAnki::start().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let mut w = worker(dir.path(), &fake);
        let mut r = request();
        r.composition.fields.insert("Absent".into(), "value".into());
        assert!(w.mine(&r, None).is_err());
        assert!(!w.parts.cache_dir.exists());
        r.composition.fields.clear();
        r.composition.senses = Some(Vec::new());
        assert!(w.mine(&r, None).is_err());
        assert!(!w.parts.cache_dir.exists());
        let lookup = w.parts.dictionaries.as_ref().unwrap().lookup().unwrap();
        let entry = lookup
            .lookup(&r.query, Options { max_results: 32 })
            .unwrap()[0]
            .clone();
        let id = compose::sense_id(&entry.glossaries[0]).unwrap();
        let mut stale = id.clone();
        stale.fingerprint.push('0');
        r.composition.senses = Some(vec![stale]);
        assert!(w.mine(&r, None).is_err());
        assert!(!w.parts.cache_dir.exists());
        assert!(
            !fake
                .requests()
                .iter()
                .any(|r| r["action"] == "storeMediaFile" || r["action"] == "addNote")
        );
        r.composition.senses = Some(vec![id]);
        r.composition
            .fields
            .insert("Sentence".into(), "literal <b>{expression}</b>".into());
        let mut config = w.parts.config.clone();
        config.destination = "anki-later".into();
        w.configure(config.clone());
        no_process_tools(&mut w);
        assert!(w.mine(&r, None).unwrap().kept.is_some());
        assert!(fake.collection().notes.is_empty());
        let card = later::list(&w.parts.later_dir).remove(0);
        let (prepared, _) = later::load(&w.parts.later_dir, &card.id).unwrap();
        assert_eq!(prepared.field_overrides, r.composition.fields);
        config.destination = "anki".into();
        w.configure(config);
        assert_eq!(w.send_later().sent, 1);
        let notes = fake.collection().notes.clone();
        assert_eq!(notes.len(), 1);
        assert_eq!(
            notes[0]
                .fields
                .iter()
                .find(|(k, _)| k == "Sentence")
                .unwrap()
                .1,
            "literal &lt;b&gt;{expression}&lt;/b&gt;"
        );
    }
}
