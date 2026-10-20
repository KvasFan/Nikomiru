// SPDX-License-Identifier: GPL-3.0-only

//! The `anki` worker ([ADR 0007]): AnkiConnect requests one at a time on a
//! thread of their own — the duplicate checks of the popup (ANK-030), the
//! mining of a card (ANK-031…ANK-039), "Open in Anki" (ANK-038). A card is
//! the popup entry composed again from the dictionaries, the frame on screen
//! and the sentence audio made in parallel, the media uploaded, then
//! `addNote` or the overwrite of the last card.
//!
//! [ADR 0007]: ../../../docs/adr/0007-mining-to-anki.md

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;

use nikomiru_anki::compose::{self, DictionaryFields, SceneFields, Source};
use nikomiru_anki::link::{self, SourceLink};
use nikomiru_anki::{AnkiError, Client, DuplicateScope, Note, lapis};
use nikomiru_core::config::AnkiConfig;
use nikomiru_core::{Cancel, Clock, Localized};
use nikomiru_dict::css::Sheet;
use nikomiru_dict::lookup::Options;
use nikomiru_library::{DictionaryManager, Library, MinedCard};
use nikomiru_media::clip::{self, AudioClip};
use nikomiru_media::process::run;
use nikomiru_media::tools::{AudioFormat, ImageFormat, Search, Tools, probe};
use nikomiru_ui::i18n::Translator;
use nikomiru_ui::mining::{
    AnkiLists, AnkiService, BrowseTarget, CardAction, CheckRequest, MineFailure, MineRequest, Mined,
};

/// The pause of the duplicate checks after a transport error (ANK-030).
pub const CHECK_PAUSE: Duration = Duration::from_secs(30);

/// How long mpv may take to save the frame.
const FRAME_TIMEOUT: Duration = Duration::from_secs(10);

/// Saves the frame on screen, without subtitles, as a PNG (ANK-034).
pub trait Frames: Send {
    /// Writes `path` and returns once it is written.
    fn save(&self, path: &Path) -> Result<(), String>;
}

impl Frames for nikomiru_player::PlayerHandle {
    fn save(&self, path: &Path) -> Result<(), String> {
        let reply = self.screenshot(path).map_err(|e| e.to_string())?;
        match reply.recv_timeout(FRAME_TIMEOUT) {
            Ok(result) => result.map_err(|e| e.to_string()),
            Err(e) => Err(e.to_string()),
        }
    }
}

/// What the worker needs.
pub struct Parts {
    /// `[anki]` of the settings.
    pub config: AnkiConfig,
    /// `[paths] ffmpeg` of the settings (EXT-001).
    pub ffmpeg: String,
    /// The interface strings, for the messages.
    pub translator: Translator,
    /// The dictionaries, to compose the cards.
    pub dictionaries: Option<Arc<DictionaryManager>>,
    /// Headword groups of a lookup, as the popup asks (the entry indexes
    /// must match).
    pub max_results: usize,
    /// The frame on screen.
    pub frames: Box<dyn Frames>,
    /// Where the media are made (`$XDG_CACHE_HOME/nikomiru/mining`).
    pub cache_dir: PathBuf,
    /// The library, for `mined_cards`.
    pub library: Option<Arc<Library>>,
    /// Time, for the pause of the checks and the records.
    pub clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for Parts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Parts")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

type Checked = Box<dyn FnOnce(Option<Vec<bool>>) + Send>;
type MineDone = Box<dyn FnOnce(Result<Mined, MineFailure>) + Send>;
type Browsed = Box<dyn FnOnce(Result<(), String>) + Send>;
type Listed = Box<dyn FnOnce(Result<AnkiLists, String>) + Send>;
type LapisMade = Box<dyn FnOnce(Result<bool, String>) + Send>;
/// Renders the field values for a list of note fields.
type Render<'a> = dyn Fn(&[String]) -> Result<Vec<(String, String)>, compose::FieldsError> + 'a;

enum Job {
    Check(CheckRequest, Checked),
    Mine(Box<MineRequest>, MineDone),
    Browse(BrowseTarget, Browsed),
    Refresh(Box<AnkiConfig>, Listed),
    CreateLapis(Box<AnkiConfig>, LapisMade),
    Configure(Box<AnkiConfig>),
}

/// The service: the jobs go to the worker thread, in order.
pub struct Anki {
    jobs: Option<mpsc::Sender<Job>>,
    worker: Option<JoinHandle<()>>,
    unavailable: String,
}

impl std::fmt::Debug for Anki {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Anki").finish_non_exhaustive()
    }
}

impl Drop for Anki {
    /// The worker may hold the library: it ends before the process does.
    fn drop(&mut self) {
        drop(self.jobs.take());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Anki {
    /// Starts the worker.
    pub fn start(parts: Parts) -> Self {
        let unavailable = parts.translator.tr("error-anki-unavailable", &[]);
        let (jobs, rx) = mpsc::channel::<Job>();
        let spawned = std::thread::Builder::new()
            .name("nikomiru-anki".into())
            .spawn(move || {
                let mut worker = Worker::new(parts);
                for job in rx {
                    match job {
                        Job::Check(request, done) => done(worker.check(&request)),
                        Job::Mine(request, done) => done(worker.mine(&request)),
                        Job::Browse(target, done) => done(worker.browse(&target)),
                        Job::Refresh(config, done) => done(worker.refresh(&config)),
                        Job::CreateLapis(config, done) => done(worker.create_lapis(&config)),
                        Job::Configure(config) => worker.configure(*config),
                    }
                }
            });
        let (jobs, worker) = match spawned {
            Ok(worker) => (Some(jobs), Some(worker)),
            Err(e) => {
                tracing::error!("cannot start the Anki thread: {e}");
                (None, None)
            }
        };
        Self {
            jobs,
            worker,
            unavailable,
        }
    }

    fn send(&self, job: Job) -> Option<Job> {
        match &self.jobs {
            Some(jobs) => jobs.send(job).err().map(|e| e.0),
            None => Some(job),
        }
    }

    fn refuse(&self, job: Job) {
        match job {
            Job::Check(_, done) => done(None),
            Job::Mine(_, done) => done(Err(MineFailure {
                message: self.unavailable.clone(),
                duplicate: false,
            })),
            Job::Browse(_, done) => done(Err(self.unavailable.clone())),
            Job::Refresh(_, done) => done(Err(self.unavailable.clone())),
            Job::CreateLapis(_, done) => done(Err(self.unavailable.clone())),
            Job::Configure(_) => {}
        }
    }
}

impl AnkiService for Anki {
    fn check(&self, request: CheckRequest, done: Checked) {
        if let Some(job) = self.send(Job::Check(request, done)) {
            self.refuse(job);
        }
    }

    fn mine(&self, request: MineRequest, done: MineDone) {
        if let Some(job) = self.send(Job::Mine(Box::new(request), done)) {
            self.refuse(job);
        }
    }

    fn browse(&self, target: BrowseTarget, done: Browsed) {
        if let Some(job) = self.send(Job::Browse(target, done)) {
            self.refuse(job);
        }
    }

    fn refresh(&self, config: AnkiConfig, done: Listed) {
        if let Some(job) = self.send(Job::Refresh(Box::new(config), done)) {
            self.refuse(job);
        }
    }

    fn create_lapis(&self, config: AnkiConfig, done: LapisMade) {
        if let Some(job) = self.send(Job::CreateLapis(Box::new(config), done)) {
            self.refuse(job);
        }
    }

    fn configure(&self, config: AnkiConfig) {
        let _ = self.send(Job::Configure(Box::new(config)));
    }
}

/// A media file of a card: the name it is sent under and its bytes.
type Media = (String, Vec<u8>);

struct Worker {
    parts: Parts,
    client: Result<Client, AnkiError>,
    /// The checks are paused until then, after a transport error (ANK-030).
    paused_until: Option<Duration>,
    /// The fields of the note type, once asked.
    note_fields: Option<Vec<String>>,
    tools: Option<Tools>,
}

impl Worker {
    fn new(parts: Parts) -> Self {
        let client = Client::new(&parts.config.url, &parts.config.api_key);
        if let Err(e) = &client {
            tracing::warn!("Anki: {e}");
        }
        Self {
            parts,
            client,
            paused_until: None,
            note_fields: None,
            tools: None,
        }
    }

    fn message(&self, e: &dyn Localized) -> String {
        self.parts.translator.error(e)
    }

    fn fail(&self, e: &AnkiError) -> MineFailure {
        MineFailure {
            message: self.message(e),
            duplicate: matches!(e, AnkiError::Duplicate),
        }
    }

    fn fail_with(&self, id: &str, args: &[(&str, String)]) -> MineFailure {
        MineFailure {
            message: self.parts.translator.tr(id, args),
            duplicate: false,
        }
    }

    fn client(&self) -> Result<&Client, AnkiError> {
        self.client.as_ref().map_err(Clone::clone)
    }

    fn scope(&self) -> DuplicateScope {
        if self.parts.config.duplicate_scope == "collection" {
            DuplicateScope::Collection
        } else {
            DuplicateScope::Deck
        }
    }

    /// The note type's fields, asked once (ANK-026).
    fn note_fields(&mut self) -> Result<Vec<String>, AnkiError> {
        if let Some(fields) = &self.note_fields {
            return Ok(fields.clone());
        }
        let fields = self
            .client()?
            .model_field_names(&self.parts.config.note_type)?;
        self.note_fields = Some(fields.clone());
        Ok(fields)
    }

    /// The field templates of the note type (ANK-020), with the Lapis defaults
    /// when the user has none (ANK-012).
    fn mapping(&self, fields: &[String]) -> Vec<(String, String)> {
        let note_type = &self.parts.config.note_type;
        let mine = self
            .parts
            .config
            .field_mapping
            .get(note_type)
            .cloned()
            .unwrap_or_default();
        let mapping = if mine.is_empty() {
            lapis::with_defaults(note_type, fields, &BTreeMap::new())
        } else {
            mine
        };
        mapping.into_iter().collect()
    }

    /// The first field of a word, as the duplicate check compares it: its
    /// template with what the headword gives, the rest empty.
    fn first_field(
        &mut self,
        expression: &str,
        reading: &str,
    ) -> Result<Option<(String, String)>, AnkiError> {
        let fields = self.note_fields()?;
        let Some(first) = fields.first().cloned() else {
            return Ok(None);
        };
        let template = self
            .mapping(&fields)
            .into_iter()
            .find(|(f, _)| *f == first)
            .map(|(_, t)| t)
            .unwrap_or_default();
        let value = compose::render(&template, &|name| match name {
            "expression" => nikomiru_anki::html::escape(expression),
            "reading" => nikomiru_anki::html::escape(reading),
            "furigana-plain" => compose::furigana_plain(expression, reading),
            _ => String::new(),
        });
        Ok(Some((first, value)))
    }

    fn pause_after(&mut self, e: &AnkiError) {
        if e.is_transport() {
            self.paused_until = Some(self.parts.clock.monotonic() + CHECK_PAUSE);
        } else {
            self.paused_until = None;
        }
    }

    /// The duplicate check of a popup (ANK-030).
    fn check(&mut self, request: &CheckRequest) -> Option<Vec<bool>> {
        let loopback = self.client.as_ref().is_ok_and(Client::is_loopback);
        if !loopback {
            return None;
        }
        if self
            .paused_until
            .is_some_and(|until| self.parts.clock.monotonic() < until)
        {
            return None;
        }
        let mut notes = Vec::new();
        for (expression, reading) in &request.entries {
            match self.first_field(expression, reading) {
                Ok(Some(first)) => notes.push(Note {
                    deck: self.parts.config.deck.clone(),
                    model: self.parts.config.note_type.clone(),
                    fields: vec![first],
                    tags: Vec::new(),
                    scope: self.scope(),
                }),
                Ok(None) => return None,
                Err(e) => {
                    tracing::info!("Anki check: {e}");
                    self.pause_after(&e);
                    return None;
                }
            }
        }
        let result = self.client().and_then(|c| c.duplicates(&notes));
        match result {
            Ok(found) => {
                self.paused_until = None;
                Some(found)
            }
            Err(e) => {
                tracing::info!("Anki check: {e}");
                self.pause_after(&e);
                None
            }
        }
    }

    /// The dictionary placeholders of the entry, from the dictionaries again.
    fn dictionary_fields(&self, r: &MineRequest) -> Result<DictionaryFields, MineFailure> {
        let manager = self
            .parts
            .dictionaries
            .as_ref()
            .ok_or_else(|| self.fail_with("error-dict-unavailable", &[]))?;
        let lookup = manager.lookup().map_err(|e| MineFailure {
            message: self.message(&e),
            duplicate: false,
        })?;
        let entries = lookup
            .lookup(
                &r.query,
                Options {
                    max_results: self.parts.max_results,
                },
            )
            .map_err(|e| MineFailure {
                message: self.message(&e),
                duplicate: false,
            })?;
        let entry = entries
            .get(r.entry)
            .filter(|e| e.expression == r.expression && e.reading == r.reading)
            .ok_or_else(|| self.fail_with("error-mining-dictionaries-changed", &[]))?;
        let records = manager.list().unwrap_or_default();
        let sheets: Vec<Sheet> = lookup
            .dictionaries()
            .iter()
            .map(|d| {
                let user = records
                    .iter()
                    .find(|(id, _)| *id == d.id)
                    .map(|(_, rec)| rec.user_css.as_str())
                    .unwrap_or_default();
                Sheet::parse(&format!("{}\n{user}", d.store.info().styles))
            })
            .collect();
        let sources: Vec<Source<'_>> = lookup
            .dictionaries()
            .iter()
            .zip(&sheets)
            .map(|(d, sheet)| Source {
                title: &d.store.info().index.title,
                name: &d.name,
                sheet,
            })
            .collect();
        let dicts = lookup.dictionaries();
        Ok(compose::dictionary_fields(
            entry,
            &sources,
            &mut |i, path| {
                dicts
                    .get(i)
                    .and_then(|d| d.store.media(path).ok().flatten())
            },
        ))
    }

    fn tools(&mut self) -> Option<&Tools> {
        if self.tools.is_none() {
            match probe(&Search::from_env(&self.parts.ffmpeg)) {
                Ok(tools) => self.tools = Some(tools),
                Err(e) => tracing::warn!("ffmpeg for cards: {e}"),
            }
        }
        self.tools.as_ref()
    }

    /// Makes the card image from the frame on screen (ANK-034).
    fn image(&self, tools: &Tools, stem: &str) -> Result<Media, String> {
        let png = self.parts.cache_dir.join(format!("{stem}.png"));
        let saved = self.parts.frames.save(&png);
        let preferred =
            ImageFormat::from_config(&self.parts.config.image_format).unwrap_or(ImageFormat::Jpeg);
        let result = saved.and_then(|()| {
            let format = tools
                .image_format(preferred)
                .ok_or_else(|| "ffmpeg has no image encoder".to_string())?;
            let ext = clip::image_extension(format);
            let out = self.parts.cache_dir.join(format!("{stem}.{ext}"));
            let job = clip::image_job(
                &tools.ffmpeg,
                &png,
                &out,
                format,
                self.parts.config.image_quality_level,
            );
            run(&job, &Cancel::new()).map_err(|e| e.to_string())?;
            let bytes = std::fs::read(&out).map_err(|e| e.to_string());
            let _ = std::fs::remove_file(&out);
            let bytes = bytes?;
            Ok((nikomiru_anki::media::name(&bytes, ext), bytes))
        });
        let _ = std::fs::remove_file(&png);
        result
    }

    /// Makes, or overwrites, a card (ANK-031…ANK-039).
    fn mine(&mut self, r: &MineRequest) -> Result<Mined, MineFailure> {
        let dict = self.dictionary_fields(r)?;
        if let Err(e) = std::fs::create_dir_all(&self.parts.cache_dir) {
            return Err(self.fail_with("error-mining-media", &[("reason", e.to_string())]));
        }
        let tools = self
            .tools()
            .cloned()
            .ok_or_else(|| self.fail_with("error-mining-media", &[("reason", "ffmpeg".into())]))?;
        let stem = link::new_source_id();
        // audio and image in parallel (ANK-035)
        let (config, cache_dir) = (&self.parts.config, &self.parts.cache_dir);
        let (audio, image) = std::thread::scope(|s| {
            let audio = r.clip.map(|clip| {
                let (tools, stem) = (&tools, &stem);
                s.spawn(move || sentence_audio(tools, config, cache_dir, r, clip, stem))
            });
            let image = self.image(&tools, &stem);
            let audio = audio.map(|a| a.join().unwrap_or_else(|_| Err("audio thread".into())));
            (audio, image)
        });
        let audio = match audio {
            Some(Ok(audio)) => Some(audio),
            Some(Err(reason)) => {
                tracing::warn!("sentence audio: {reason}");
                return Err(self.fail_with("error-mining-no-audio", &[]));
            }
            None => None,
        };
        let image = image
            .map_err(|reason| tracing::warn!("card image: {reason}"))
            .ok();
        let fields = self.note_fields().map_err(|e| self.fail(&e))?;
        let mapping = self.mapping(&fields);
        let (start_ms, end_ms) = r.clip.unwrap_or((r.position_ms, r.position_ms));
        let source_id = link::new_source_id();
        let link = r.uid.as_ref().map(|uid| SourceLink {
            uid: uid.clone(),
            episode: 0,
            start_ms: u32::try_from(start_ms.max(0)).unwrap_or(u32::MAX),
            end_ms: u32::try_from(end_ms.max(0)).unwrap_or(u32::MAX),
            source_id: source_id.clone(),
        });
        let length = r.matched.chars().count();
        let sentence = compose::sentence(&r.sentence, r.offset, length, &r.matched);
        let scene = |audio: Option<&str>, image: Option<&str>| SceneFields {
            sentence: sentence.clone(),
            cue_sentence: sentence.clone(),
            sentence_audio: audio.map(|a| format!("[sound:{a}]")).unwrap_or_default(),
            card_image: image
                .map(|i| format!(r#"<img src="{}">"#, nikomiru_anki::html::escape(i)))
                .unwrap_or_default(),
            audio: String::new(),
            document_title: nikomiru_anki::html::escape(&r.title),
            source_link: link
                .as_ref()
                .map(|l| l.to_html(&r.title))
                .unwrap_or_default(),
            clip_timestamp: if r.clip.is_some() {
                compose::clip_timestamp(start_ms, end_ms)
            } else {
                String::new()
            },
        };
        let keep_empty = matches!(r.action, CardAction::Overwrite(_));
        let render = |audio: Option<&str>, image: Option<&str>, fields: &[String]| {
            let scene = scene(audio, image);
            compose::fields(
                fields,
                &mapping,
                &|name| compose::value(name, &dict, &scene),
                keep_empty,
            )
        };
        // the fields must hold before anything goes to Anki (ANK-026)
        render(
            audio.as_ref().map(|a| a.0.as_str()),
            image.as_ref().map(|i| i.0.as_str()),
            &fields,
        )
        .map_err(|e| self.fields_failure(&e))?;

        // the media: which are new, then the upload (ANK-036)
        let mut media: Vec<Media> = dict.media.clone();
        media.extend(audio.clone());
        media.extend(image.clone());
        let client = self.client().map_err(|e| self.fail(&e))?;
        let names: Vec<String> = media.iter().map(|(n, _)| n.clone()).collect();
        let existed = client.media_exist(&names).map_err(|e| self.fail(&e))?;
        let mut stored: Vec<(String, String, bool)> = Vec::new();
        for ((name, bytes), existed) in media.iter().zip(existed) {
            match client.store_media(name, bytes) {
                Ok(actual) => stored.push((name.clone(), actual, !existed)),
                Err(e) => {
                    rollback(client, &stored);
                    return Err(self.fail(&e));
                }
            }
        }
        let actual = |name: &str| {
            stored
                .iter()
                .find(|(n, _, _)| n == name)
                .map_or_else(|| name.to_string(), |(_, a, _)| a.clone())
        };
        let audio_name = audio.as_ref().map(|a| actual(&a.0));
        let image_name = image.as_ref().map(|i| actual(&i.0));
        let tags = compose::tags(&self.parts.config.tags);
        let outcome = match r.action {
            CardAction::Add | CardAction::AddAnyway => {
                let values = render(audio_name.as_deref(), image_name.as_deref(), &fields)
                    .map_err(|e| self.fields_failure(&e))?;
                let values = renamed(values, &stored);
                let note = Note {
                    deck: self.parts.config.deck.clone(),
                    model: self.parts.config.note_type.clone(),
                    fields: values,
                    tags,
                    scope: self.scope(),
                };
                client
                    .add_note(&note, r.action == CardAction::AddAnyway)
                    .map(|id| (id, false))
                    .map_err(OverwriteError::Anki)
            }
            CardAction::Overwrite(id) => self.overwrite(
                client,
                id,
                &|f| {
                    render(audio_name.as_deref(), image_name.as_deref(), f)
                        .map(|v| renamed(v, &stored))
                },
                &tags,
            ),
        };
        let (note_id, overwritten) = match outcome {
            Ok(done) => done,
            Err(OverwriteError::Fields(e)) => {
                rollback(client, &stored);
                return Err(self.fields_failure(&e));
            }
            Err(OverwriteError::Other(id, args)) => {
                rollback(client, &stored);
                return Err(self.fail_with(id, &args));
            }
            Err(OverwriteError::Anki(e)) => {
                // the note may exist: its media stay (ANK-003)
                if !matches!(e, AnkiError::CommitUnknown { .. }) {
                    rollback(client, &stored);
                }
                if e.is_transport() || matches!(e, AnkiError::CommitUnknown { .. }) {
                    self.pause_after(&e);
                }
                return Err(self.fail(&e));
            }
        };
        self.record(note_id, r, &source_id, (start_ms, end_ms));
        Ok(Mined {
            note_id,
            deck: self.parts.config.deck.clone(),
            overwritten,
        })
    }

    fn fields_failure(&self, e: &compose::FieldsError) -> MineFailure {
        match e {
            compose::FieldsError::EmptyFirstField => self.fail(&AnkiError::EmptyNote),
            compose::FieldsError::NoMappedField => self.fail_with("error-mining-mapping", &[]),
        }
    }

    /// Overwrites the last card (ANK-037): every field of the note, then its
    /// tags, then a check that the first field was written.
    fn overwrite(
        &self,
        client: &Client,
        id: u64,
        render: &Render<'_>,
        tags: &[String],
    ) -> Result<(u64, bool), OverwriteError> {
        let note = client
            .notes_info(&[id])
            .map_err(OverwriteError::Anki)?
            .into_iter()
            .next()
            .flatten()
            .ok_or(OverwriteError::Other("error-mining-note-gone", Vec::new()))?;
        if note.model != self.parts.config.note_type {
            return Err(OverwriteError::Other(
                "error-mining-other-note-type",
                vec![("name", note.model)],
            ));
        }
        let names: Vec<String> = note.fields.iter().map(|(n, _)| n.clone()).collect();
        let values = render(&names).map_err(OverwriteError::Fields)?;
        client
            .update_note_fields(id, &values)
            .map_err(OverwriteError::Anki)?;
        client.add_tags(&[id], tags).map_err(OverwriteError::Anki)?;
        let after = client
            .notes_info(&[id])
            .map_err(OverwriteError::Anki)?
            .into_iter()
            .next()
            .flatten();
        let first_written = match (after, values.first()) {
            (Some(after), Some((name, value))) => after
                .fields
                .iter()
                .any(|(n, v)| n == name && v.trim() == value.trim()),
            _ => false,
        };
        if !first_written {
            return Err(OverwriteError::Other(
                "error-mining-overwrite-lost",
                Vec::new(),
            ));
        }
        Ok((id, true))
    }

    /// The `mined_cards` record (ANK-039).
    fn record(&self, note_id: u64, r: &MineRequest, source_id: &str, (start, end): (i64, i64)) {
        let Some(library) = &self.parts.library else {
            return;
        };
        let created_at = self
            .parts
            .clock
            .now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as i64);
        let card = MinedCard {
            expression: r.expression.clone(),
            reading: r.reading.clone(),
            sentence: r.sentence.clone(),
            video_uid: r.uid.clone().unwrap_or_default(),
            start_ms: u32::try_from(start.max(0)).unwrap_or(u32::MAX),
            end_ms: u32::try_from(end.max(0)).unwrap_or(u32::MAX),
            source_id: source_id.to_string(),
            created_at,
        };
        if let Err(e) = library.put_mined_card(note_id, &card) {
            tracing::warn!("mined_cards: {e}");
        }
    }

    /// The connection test of the settings (ANK-006), with their address.
    fn refresh(&mut self, config: &AnkiConfig) -> Result<AnkiLists, String> {
        let client = Client::new(&config.url, &config.api_key).map_err(|e| self.message(&e))?;
        let state = lapis::refresh(&client, &config.note_type).map_err(|e| self.message(&e))?;
        Ok(AnkiLists {
            version: state.version,
            decks: state.decks,
            models: state.models,
            model: config.note_type.clone(),
            fields: state.fields,
        })
    }

    /// "Create Lapis deck" (ANK-011), with the settings' address.
    fn create_lapis(&mut self, config: &AnkiConfig) -> Result<bool, String> {
        let client = Client::new(&config.url, &config.api_key).map_err(|e| self.message(&e))?;
        let setup = lapis::create(&client).map_err(|e| self.message(&e))?;
        Ok(setup == lapis::Setup::Created)
    }

    /// New settings: a new client, the note type's fields asked again, the
    /// pause of the checks lifted.
    fn configure(&mut self, config: AnkiConfig) {
        self.client = Client::new(&config.url, &config.api_key);
        self.parts.config = config;
        self.note_fields = None;
        self.paused_until = None;
    }

    /// "Open in Anki" (ANK-038).
    fn browse(&mut self, target: &BrowseTarget) -> Result<(), String> {
        let query = match target {
            BrowseTarget::Note(id) => format!("nid:{id}"),
            BrowseTarget::Word {
                expression,
                reading,
            } => {
                let first = self
                    .first_field(expression, reading)
                    .map_err(|e| self.message(&e))?
                    .ok_or_else(|| self.parts.translator.tr("error-mining-not-found", &[]))?;
                let client = self.client().map_err(|e| self.message(&e))?;
                let models = client.model_names_and_ids().map_err(|e| self.message(&e))?;
                let value = first.1.replace('\\', "\\\\").replace('"', "\\\"");
                let dupes: Vec<String> = models
                    .iter()
                    .map(|(_, id)| format!("\"dupe:{id},{value}\""))
                    .collect();
                let mut query = format!("({})", dupes.join(" OR "));
                if self.scope() == DuplicateScope::Deck {
                    query.push(' ');
                    query.push_str(&nikomiru_anki::client::deck_query(&self.parts.config.deck));
                }
                let ids = client.find_notes(&query).map_err(|e| self.message(&e))?;
                if ids.is_empty() {
                    return Err(self.parts.translator.tr("error-mining-not-found", &[]));
                }
                let ids: Vec<String> = ids.iter().map(u64::to_string).collect();
                format!("nid:{}", ids.join(","))
            }
        };
        let client = self.client().map_err(|e| self.message(&e))?;
        client.gui_browse(&query).map_err(|e| self.message(&e))
    }
}

enum OverwriteError {
    Fields(compose::FieldsError),
    Other(&'static str, Vec<(&'static str, String)>),
    Anki(AnkiError),
}

/// The field values with the names Anki gave the media, where it changed
/// one.
fn renamed(
    values: Vec<(String, String)>,
    stored: &[(String, String, bool)],
) -> Vec<(String, String)> {
    values
        .into_iter()
        .map(|(field, mut value)| {
            for (asked, actual, _) in stored {
                if asked != actual {
                    value = value.replace(asked.as_str(), actual);
                }
            }
            (field, value)
        })
        .collect()
}

/// Deletes the media this attempt added (ANK-036); the ones that were there
/// before stay.
fn rollback(client: &Client, stored: &[(String, String, bool)]) {
    for (_, actual, new) in stored {
        if *new && let Err(e) = client.delete_media(actual) {
            tracing::warn!("deleting {actual}: {e}");
        }
    }
}

/// Makes the sentence audio (ANK-033): its Anki name and bytes.
fn sentence_audio(
    tools: &Tools,
    config: &AnkiConfig,
    cache_dir: &Path,
    r: &MineRequest,
    (start, end): (i64, i64),
    stem: &str,
) -> Result<Media, String> {
    let stream = r.audio_stream.ok_or_else(|| "no audio track".to_string())?;
    let preferred = AudioFormat::from_config(&config.audio_format).unwrap_or(AudioFormat::AacM4a);
    let format = tools
        .audio_format(preferred)
        .ok_or_else(|| "ffmpeg has no audio encoder".to_string())?;
    let ext = clip::audio_extension(format);
    let out = cache_dir.join(format!("{stem}.{ext}"));
    let job = AudioClip {
        input: r.video.clone(),
        stream: Some(stream),
        start_ms: start,
        end_ms: end,
        format,
        quality: config.audio_quality_level,
    }
    .job(&tools.ffmpeg, &out);
    run(&job, &Cancel::new()).map_err(|e| e.to_string())?;
    let bytes = std::fs::read(&out).map_err(|e| e.to_string());
    let _ = std::fs::remove_file(&out);
    let bytes = bytes?;
    if bytes.is_empty() {
        return Err("empty audio".to_string());
    }
    Ok((nikomiru_anki::media::name(&bytes, ext), bytes))
}
