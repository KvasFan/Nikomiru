// SPDX-License-Identifier: GPL-3.0-only

//! The `anki` worker ([ADR 0007]): AnkiConnect requests one at a time on a
//! thread of their own — the duplicate checks of the popup (ANK-030), the
//! mining of a card (ANK-031…ANK-039), "Open in Anki" (ANK-038). A card is
//! the popup entry composed again from the dictionaries, the frame saved at
//! the click and the sentence audio made in parallel, the media uploaded, then
//! `addNote` or the overwrite of the last card.
//!
//! [ADR 0007]: ../../../docs/adr/0007-mining-to-anki.md

use std::cell::Cell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
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
    AnkiLists, AnkiService, BrowseTarget, CardAction, CheckRequest, FrameChoice, LapisMade,
    MineFailure, MineRequest, Mined, PendingFrame,
};

/// The pause of the duplicate checks after a transport error (ANK-030).
pub const CHECK_PAUSE: Duration = Duration::from_secs(30);

/// How long the player may take to save the frame of a card.
pub const FRAME_TIMEOUT: Duration = Duration::from_secs(10);

/// The age from which a file in the media folder is left from an attempt cut
/// short (ADR 0007 §20): no attempt lasts as long.
pub const STALE_MEDIA: Duration = Duration::from_secs(600);

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
    /// must match): the settings change it (SET-002).
    pub max_results: Arc<AtomicUsize>,
    /// Where the media are made (`$XDG_CACHE_HOME/nikomiru/mining`), the
    /// frames of the clicks among them.
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
type LapisDone = Box<dyn FnOnce(Result<LapisMade, String>) + Send>;
/// Renders the field values for a list of note fields.
type Render<'a> = dyn Fn(&[String]) -> Result<Vec<(String, String)>, compose::FieldsError> + 'a;

enum Job {
    Check(CheckRequest, Checked),
    Mine(Box<MineRequest>, Option<PendingFrame>, MineDone),
    Browse(BrowseTarget, Browsed),
    Refresh(Box<AnkiConfig>, Listed),
    CreateLapis(Box<AnkiConfig>, LapisDone),
    Configure(Box<AnkiConfig>),
}

/// The service: the jobs go to the worker thread, in order.
pub struct Anki {
    jobs: Option<mpsc::Sender<Job>>,
    worker: Option<JoinHandle<()>>,
    /// Set when Nikomiru closes: the jobs still queued are not run.
    closing: Arc<AtomicBool>,
    unavailable: String,
}

impl std::fmt::Debug for Anki {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Anki").finish_non_exhaustive()
    }
}

impl Drop for Anki {
    /// The worker may hold the library: it ends before the process does,
    /// after the job it is on; the queued ones are dropped, so the window
    /// does not wait for them.
    fn drop(&mut self) {
        self.closing.store(true, Ordering::SeqCst);
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
        let closing = Arc::new(AtomicBool::new(false));
        let (jobs, rx) = mpsc::channel::<Job>();
        let stop = Arc::clone(&closing);
        let spawned = std::thread::Builder::new()
            .name("nikomiru-anki".into())
            .spawn(move || {
                sweep(&parts.cache_dir);
                let mut worker = Worker::new(parts);
                for job in rx {
                    if stop.load(Ordering::SeqCst) {
                        break;
                    }
                    match job {
                        Job::Check(request, done) => done(worker.check(&request)),
                        Job::Mine(request, frame, done) => done(worker.mine(&request, frame)),
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
            closing,
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
            Job::Mine(_, _, done) => done(Err(MineFailure {
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

    fn mine(&self, request: MineRequest, frame: Option<PendingFrame>, done: MineDone) {
        if let Some(job) = self.send(Job::Mine(Box::new(request), frame, done)) {
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

    fn create_lapis(&self, config: AnkiConfig, done: LapisDone) {
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
    paused_until: Cell<Option<Duration>>,
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
            paused_until: Cell::new(None),
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
            .model_field_names(&self.parts.config.note_type);
        let fields = self.heard(fields)?;
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

    /// Notes how a request to Anki ended, for the pause of the checks
    /// (ANK-030, ADR 0007 §22): an error that left Anki unanswered starts it,
    /// any answer ends it.
    fn heard<T>(&self, result: Result<T, AnkiError>) -> Result<T, AnkiError> {
        let unanswered = result
            .as_ref()
            .err()
            .is_some_and(|e| e.is_transport() || matches!(e, AnkiError::CommitUnknown { .. }));
        self.paused_until
            .set(unanswered.then(|| self.parts.clock.monotonic() + CHECK_PAUSE));
        result
    }

    /// [`Worker::heard`] for a request of the settings, if it went to the
    /// AnkiConnect the worker uses.
    fn heard_from<T>(&self, client: &Client, result: Result<T, AnkiError>) -> Result<T, AnkiError> {
        if self.client.as_ref().is_ok_and(|c| c.url() == client.url()) {
            self.heard(result)
        } else {
            result
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
            .get()
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
                    return None;
                }
            }
        }
        let found = self.client().map(|c| c.duplicates(&notes));
        match found.and_then(|found| self.heard(found)) {
            Ok(found) => Some(found),
            Err(e) => {
                tracing::info!("Anki check: {e}");
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
                    max_results: self.parts.max_results.load(Ordering::Relaxed),
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

    /// Makes the card's picture (ANK-034, ANK-040, ANK-042): the sentence as
    /// an animated picture, when that is the format and there is a sentence;
    /// else the frame of the click, whose PNG goes with `frame`; the frame at
    /// a time of the video, taken with ffmpeg; or none.
    fn image(
        &self,
        tools: &Tools,
        r: &MineRequest,
        frame: Option<PendingFrame>,
        stem: &str,
    ) -> Result<Option<Media>, String> {
        if r.frame == FrameChoice::None && r.picture.is_none() {
            return Ok(None);
        }
        let preferred =
            ImageFormat::from_config(&self.parts.config.image_format).unwrap_or(ImageFormat::Jpeg);
        let format = tools
            .image_format(preferred)
            .ok_or_else(|| "ffmpeg has no image encoder".to_string())?;
        if let Some(bytes) = &r.picture {
            // the part of a manga's page, in the format of the other cards
            let made = self.parts.cache_dir.join(format!("{stem}-page.jpg"));
            std::fs::write(&made, bytes).map_err(|e| e.to_string())?;
            let media = self.encode(tools, &made, format.still(), stem);
            let _ = std::fs::remove_file(&made);
            return media.map(Some);
        }
        if format.is_animated()
            && let Some(clip) = r.clip
        {
            // the click's frame is not needed: dropping it removes its file
            drop(frame);
            return self.animate(tools, r, clip, format, stem).map(Some);
        }
        let format = format.still();
        match r.frame {
            FrameChoice::None => Ok(None),
            FrameChoice::Click => {
                let mut frame = frame.ok_or_else(|| "the player was not asked".to_string())?;
                match frame.written(FRAME_TIMEOUT) {
                    Ok(png) => self.encode(tools, png, format, stem).map(Some),
                    // mpv 0.41 cannot read back a frame the GPU decoded: the
                    // same moment, from the file (ANK-034)
                    Err(e) => {
                        tracing::info!("the player's frame: {e}; ffmpeg instead");
                        self.frame_at(tools, r, r.position_ms, format, stem)
                    }
                }
            }
            FrameChoice::At(ms) => self.frame_at(tools, r, ms, format, stem),
        }
    }

    /// The frame at `ms` of the video, taken with ffmpeg, as the card's picture.
    fn frame_at(
        &self,
        tools: &Tools,
        r: &MineRequest,
        ms: i64,
        format: ImageFormat,
        stem: &str,
    ) -> Result<Option<Media>, String> {
        let png = self.parts.cache_dir.join(format!("{stem}-frame.png"));
        let job = clip::frame_job(&tools.ffmpeg, &r.video, ms, &png);
        let made = run(&job, &Cancel::new())
            .map_err(|e| e.to_string())
            .and_then(|_| self.encode(tools, &png, format, stem));
        let _ = std::fs::remove_file(&png);
        made.map(Some)
    }

    /// The PNG `png` as the card's picture, in `format` and the size of the
    /// settings.
    fn encode(
        &self,
        tools: &Tools,
        png: &Path,
        format: ImageFormat,
        stem: &str,
    ) -> Result<Media, String> {
        let ext = clip::image_extension(format);
        let out = self.parts.cache_dir.join(format!("{stem}.{ext}"));
        let job = clip::image_job(
            &tools.ffmpeg,
            png,
            &out,
            format,
            self.parts.config.image_quality_level,
        );
        read_output(&job, &out, ext)
    }

    /// The sentence `clip` of the video as an animated picture (ANK-042).
    fn animate(
        &self,
        tools: &Tools,
        r: &MineRequest,
        clip: (i64, i64),
        format: ImageFormat,
        stem: &str,
    ) -> Result<Media, String> {
        let ext = clip::image_extension(format);
        let out = self.parts.cache_dir.join(format!("{stem}.{ext}"));
        let level = self.parts.config.image_quality_level;
        let job = if format == ImageFormat::WebmVideo {
            // without Opus, or with the sound left out, a silent video
            let sound = r
                .audio_stream
                .filter(|_| r.audio && tools.has_encoder("libopus"))
                .map(|stream| (r.audio_input.as_path(), stream));
            clip::video_job(&tools.ffmpeg, &r.video, sound, clip, &out, level)
        } else {
            clip::animated_job(&tools.ffmpeg, &r.video, clip, &out, format, level)
        };
        read_output(&job, &out, ext)
    }

    /// Makes, or overwrites, a card (ANK-031…ANK-039).
    fn mine(&mut self, r: &MineRequest, frame: Option<PendingFrame>) -> Result<Mined, MineFailure> {
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
            let audio = r.clip.filter(|_| r.audio).map(|clip| {
                let (tools, stem) = (&tools, &stem);
                s.spawn(move || sentence_audio(tools, config, cache_dir, r, clip, stem))
            });
            let image = self.image(&tools, r, frame, &stem);
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
        let image = image.unwrap_or_else(|reason| {
            tracing::warn!("card image: {reason}");
            None
        });
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
        let sentence = compose::sentence(&r.sentence, r.offset, &r.matched);
        let scene = |audio: Option<&str>, image: Option<&str>| SceneFields {
            sentence: sentence.clone(),
            cue_sentence: sentence.clone(),
            sentence_audio: audio.map(|a| format!("[sound:{a}]")).unwrap_or_default(),
            card_image: image.map(compose::card_image).unwrap_or_default(),
            audio: String::new(),
            document_title: nikomiru_anki::html::escape(&r.title),
            // without the library there is no link: the title, as in Fushi
            source_link: link.as_ref().map_or_else(
                || nikomiru_anki::html::escape(&r.title),
                |l| l.to_html(&r.title),
            ),
            clip_timestamp: if r.clip.is_some() {
                compose::clip_timestamp(start_ms, end_ms)
            } else {
                String::new()
            },
        };
        let keep_empty = matches!(r.action, CardAction::Overwrite(_));
        // a Lapis-like note reads some fields through `text:` in its scripts
        let lapis_like = lapis::is_lapis(&self.parts.config.note_type, &fields);
        let render = |audio: Option<&str>, image: Option<&str>, fields: &[String]| {
            let scene = scene(audio, image);
            compose::fields(
                fields,
                &mapping,
                &|name| compose::value(name, &dict, &scene),
                keep_empty,
            )
            .map(|values| {
                if lapis_like {
                    compose::lapis_safe(values)
                } else {
                    values
                }
            })
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
        let existed = self
            .heard(client.media_exist(&names))
            .map_err(|e| self.fail(&e))?;
        let mut stored: Vec<(String, String, bool)> = Vec::new();
        for ((name, bytes), existed) in media.iter().zip(existed) {
            match self.heard(client.store_media(name, bytes)) {
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
        // ANK-027
        let series = r.uid.as_deref().and_then(|uid| {
            self.parts
                .library
                .as_ref()?
                .series_title_of_video(uid)
                .inspect_err(|e| tracing::warn!("library: {e}"))
                .ok()
                .flatten()
        });
        let tags = compose::tags(&self.parts.config.tags, series.as_deref());
        let outcome = match r.action {
            CardAction::Add | CardAction::AddAnyway => {
                match render(audio_name.as_deref(), image_name.as_deref(), &fields) {
                    Ok(values) => {
                        let note = Note {
                            deck: self.parts.config.deck.clone(),
                            model: self.parts.config.note_type.clone(),
                            fields: renamed(values, &stored),
                            tags,
                            scope: self.scope(),
                        };
                        let added =
                            self.heard(client.add_note(&note, r.action == CardAction::AddAnyway));
                        // the client knows when the note may exist (ANK-003)
                        added.map(|id| (id, false)).map_err(|e| match e {
                            AnkiError::CommitUnknown { .. } => NoteError::Unknown(self.fail(&e)),
                            _ => NoteError::Unchanged(self.fail(&e)),
                        })
                    }
                    Err(e) => Err(NoteError::Unchanged(self.fields_failure(&e))),
                }
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
            Err(NoteError::Unchanged(failure)) => {
                rollback(client, &stored);
                return Err(failure);
            }
            // the note may exist, or have changed: its media stay (ANK-003)
            Err(NoteError::Unknown(failure)) => return Err(failure),
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
    /// tags, then a check that the first field was written. Once
    /// `updateNoteFields` may have reached Anki, a failure leaves the outcome
    /// unknown.
    fn overwrite(
        &self,
        client: &Client,
        id: u64,
        render: &Render<'_>,
        tags: &[String],
    ) -> Result<(u64, bool), NoteError> {
        let unchanged = |e: AnkiError| NoteError::Unchanged(self.fail(&e));
        let note = self
            .heard(client.notes_info(&[id]))
            .map_err(unchanged)?
            .into_iter()
            .next()
            .flatten()
            .ok_or_else(|| NoteError::Unchanged(self.fail_with("error-mining-note-gone", &[])))?;
        if note.model != self.parts.config.note_type {
            return Err(NoteError::Unchanged(self.fail_with(
                "error-mining-other-note-type",
                &[("name", note.model)],
            )));
        }
        let names: Vec<String> = note.fields.iter().map(|(n, _)| n.clone()).collect();
        let values = render(&names).map_err(|e| NoteError::Unchanged(self.fields_failure(&e)))?;
        let unknown = |e: AnkiError| {
            tracing::warn!("overwriting note {id}: {e}");
            NoteError::Unknown(self.fail_with("error-mining-overwrite-unknown", &[]))
        };
        self.heard(client.update_note_fields(id, &values))
            .map_err(|e| {
                if may_have_changed(&e) {
                    unknown(e)
                } else {
                    unchanged(e)
                }
            })?;
        // the fields are written from here
        self.heard(client.add_tags(&[id], tags)).map_err(unknown)?;
        let after = self
            .heard(client.notes_info(&[id]))
            .map_err(unknown)?
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
            // Anki's editor may write the old fields back, or keep some new
            // ones: the media stay
            return Err(NoteError::Unknown(
                self.fail_with("error-mining-overwrite-lost", &[]),
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
        let state = lapis::refresh(&client, &config.note_type);
        let state = self
            .heard_from(&client, state)
            .map_err(|e| self.message(&e))?;
        Ok(AnkiLists {
            version: state.version,
            decks: state.decks,
            models: state.models,
            deck: config.deck.clone(),
            model: config.note_type.clone(),
            fields: state.fields,
        })
    }

    /// "Create Lapis deck" (ANK-011), with the settings' address.
    fn create_lapis(&mut self, config: &AnkiConfig) -> Result<LapisMade, String> {
        let client = Client::new(&config.url, &config.api_key).map_err(|e| self.message(&e))?;
        let made = lapis::create(&client);
        let made = self
            .heard_from(&client, made)
            .map_err(|e| self.message(&e))?;
        Ok(LapisMade {
            created: made.setup == lapis::Setup::Created,
            deck: made.deck,
            note_type: made.note_type,
        })
    }

    /// New settings: a new client, the note type's fields asked again, the
    /// pause of the checks lifted.
    fn configure(&mut self, config: AnkiConfig) {
        self.client = Client::new(&config.url, &config.api_key);
        self.parts.config = config;
        self.note_fields = None;
        self.paused_until.set(None);
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
                let models = self
                    .heard(client.model_names_and_ids())
                    .map_err(|e| self.message(&e))?;
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
                let ids = self
                    .heard(client.find_notes(&query))
                    .map_err(|e| self.message(&e))?;
                if ids.is_empty() {
                    return Err(self.parts.translator.tr("error-mining-not-found", &[]));
                }
                let ids: Vec<String> = ids.iter().map(u64::to_string).collect();
                format!("nid:{}", ids.join(","))
            }
        };
        let client = self.client().map_err(|e| self.message(&e))?;
        self.heard(client.gui_browse(&query))
            .map_err(|e| self.message(&e))
    }
}

/// Why a note was not made or overwritten.
enum NoteError {
    /// Anki has it as it was: the new media are deleted again (ANK-036).
    Unchanged(MineFailure),
    /// It may exist, or have changed: its media stay (ANK-003).
    Unknown(MineFailure),
}

/// Whether an idempotent request that failed with `e` may still have been
/// carried out: it may have reached Anki, and no answer of AnkiConnect says
/// otherwise.
fn may_have_changed(e: &AnkiError) -> bool {
    matches!(
        e,
        AnkiError::Timeout { .. }
            | AnkiError::Http { .. }
            | AnkiError::Protocol { .. }
            | AnkiError::CommitUnknown { .. }
    )
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

/// Removes what attempts cut short left in the media folder (ADR 0007 §20):
/// the files older than any attempt lasts. Newer ones may be another
/// window's.
fn sweep(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let stale = entry
            .metadata()
            .and_then(|m| m.modified())
            .is_ok_and(|t| t.elapsed().is_ok_and(|age| age > STALE_MEDIA));
        if stale
            && entry.file_type().is_ok_and(|t| t.is_file())
            && let Err(e) = std::fs::remove_file(entry.path())
        {
            tracing::warn!("{}: {e}", entry.path().display());
        }
    }
}

/// Makes the sentence audio (ANK-033): its Anki name and bytes.
/// Runs `job`, then reads and removes its output `out`: a card's media.
fn read_output(job: &nikomiru_media::process::Job, out: &Path, ext: &str) -> Result<Media, String> {
    run(job, &Cancel::new()).map_err(|e| e.to_string())?;
    let bytes = std::fs::read(out).map_err(|e| e.to_string());
    let _ = std::fs::remove_file(out);
    let bytes = bytes?;
    Ok((nikomiru_anki::media::name(&bytes, ext), bytes))
}

fn sentence_audio(
    tools: &Tools,
    config: &AnkiConfig,
    cache_dir: &Path,
    r: &MineRequest,
    (start, end): (i64, i64),
    stem: &str,
) -> Result<Media, String> {
    let stream = r.audio_stream.ok_or_else(|| "no audio track".to_string())?;
    if end <= start.max(0) {
        return Err("the clip is empty".to_string());
    }
    let preferred = AudioFormat::from_config(&config.audio_format).unwrap_or(AudioFormat::AacM4a);
    let format = tools
        .audio_format(preferred)
        .ok_or_else(|| "ffmpeg has no audio encoder".to_string())?;
    let ext = clip::audio_extension(format);
    let out = cache_dir.join(format!("{stem}.{ext}"));
    let job = AudioClip {
        input: r.audio_input.clone(),
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
