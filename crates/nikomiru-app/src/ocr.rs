// SPDX-License-Identifier: GPL-3.0-only

//! The reader of a manga's pages (MNG-003, MNG-004): the model, fetched once
//! on request and checked before it goes in its place, then loaded the first
//! time a page is read and kept for the next. Pages are read ahead, a whole
//! page at a time, on a thread of their own (MNG-011).

use std::collections::VecDeque;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use nikomiru_books::mokuro::{Page, Region};
use nikomiru_core::Localized;
use nikomiru_ocr::{DETECTOR, Detector, Engine, FILES, MODEL, ModelFile};
use nikomiru_online::error::OnlineError;
use nikomiru_online::http::{CONNECT_TIMEOUT, Http, checked_url};
use nikomiru_ui::i18n::Translator;
use nikomiru_ui::ocr::{
    OcrEvent, OcrJob, OcrService, PageJob, PageReports, Reading, Recognized, Reports,
};
use sha2::{Digest, Sha256};

/// Who serves the model, for the errors.
const PROVIDER: &str = "Hugging Face";
/// Time for the whole model on a slow line.
const FETCH_TIMEOUT: Duration = Duration::from_secs(3 * 3600);
/// How often the progress is told.
const REPORT_EVERY: u64 = 2 << 20;

/// How long the pages' thread waits for more before it ends.
const PAGES_IDLE: Duration = Duration::from_secs(60);

/// The worker.
#[derive(Debug)]
pub struct OcrWorker {
    dir: PathBuf,
    engine: Arc<Mutex<Option<Arc<Engine>>>>,
    pages: Arc<Pages>,
}

/// The pages to read ahead, and what reads them (MNG-011).
#[derive(Default)]
struct Pages {
    queue: Mutex<PageQueue>,
    more: Condvar,
    detector: Mutex<Option<Arc<Detector>>>,
    /// Parts of pages asked for by the reader, being read: they go first.
    asked: AtomicUsize,
    /// Where pages read are kept, so a manga opened again is clicked at once.
    cache: Option<PathBuf>,
}

impl std::fmt::Debug for Pages {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pages").field("cache", &self.cache).finish()
    }
}

#[derive(Default)]
struct PageQueue {
    jobs: VecDeque<PageJob>,
    done: Option<PageReports>,
    running: bool,
}

impl OcrWorker {
    /// A worker for the model in `dir`.
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            engine: Arc::default(),
            pages: Arc::default(),
        }
    }

    /// The same, keeping the pages it reads ahead in `cache` (MNG-011).
    pub fn with_cache(dir: PathBuf, cache: PathBuf) -> Self {
        Self {
            pages: Arc::new(Pages {
                cache: Some(cache),
                ..Pages::default()
            }),
            ..Self::new(dir)
        }
    }

    /// The model, loaded once for the pages and the graphic subtitles
    /// (SUB-028) alike.
    pub fn shared(&self) -> SharedEngine {
        SharedEngine {
            dir: self.dir.clone(),
            engine: self.engine.clone(),
        }
    }
}

/// The model in its folder, loaded the first time it is asked for.
#[derive(Debug, Clone)]
pub struct SharedEngine {
    dir: PathBuf,
    engine: Arc<Mutex<Option<Arc<Engine>>>>,
}

impl SharedEngine {
    /// Whether the model is there, or the tests' text stands in for it.
    pub fn available(&self) -> bool {
        test_text().is_some() || nikomiru_ocr::installed(&self.dir)
    }

    /// Whether the model is loaded already.
    pub fn loaded(&self) -> bool {
        self.engine
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some()
    }

    /// The model, loaded.
    pub fn get(&self) -> Result<Arc<Engine>, nikomiru_ocr::OcrError> {
        let mut engine = self.engine.lock().unwrap_or_else(|e| e.into_inner());
        match &*engine {
            Some(loaded) => Ok(loaded.clone()),
            None => Ok(engine.insert(Arc::new(Engine::load(&self.dir)?)).clone()),
        }
    }
}

/// Fetches `files` into `dir`, each checked before it takes its name, so
/// what is there is whole. `progress` hears the bytes so far.
pub fn install(
    http: &Http,
    files: &[ModelFile<'_>],
    dir: &Path,
    progress: &dyn Fn(u64),
) -> Result<(), OnlineError> {
    let io = |e: std::io::Error| OnlineError::Io(e.to_string());
    std::fs::create_dir_all(dir).map_err(io)?;
    let mut before = 0;
    for file in files {
        let target = dir.join(file.name);
        if std::fs::metadata(&target).is_ok_and(|m| m.len() == file.bytes) {
            before += file.bytes;
            continue;
        }
        let part = dir.join(format!("{}.part", file.name));
        let fetched = fetch(http, file, &part, before, progress)
            .and_then(|()| std::fs::rename(&part, &target).map_err(io));
        if fetched.is_err() {
            let _ = std::fs::remove_file(&part);
        }
        fetched?;
        before += file.bytes;
    }
    Ok(())
}

fn fetch(
    http: &Http,
    file: &ModelFile<'_>,
    part: &Path,
    before: u64,
    progress: &dyn Fn(u64),
) -> Result<(), OnlineError> {
    let io = |e: std::io::Error| OnlineError::Io(e.to_string());
    let mut out = std::fs::File::create(part).map_err(io)?;
    let mut hash = Sha256::new();
    let (mut seen, mut told) = (0, 0);
    http.stream(
        &checked_url(file.url)?,
        PROVIDER,
        file.bytes,
        &mut |piece| {
            out.write_all(piece).map_err(io)?;
            hash.update(piece);
            seen += piece.len() as u64;
            if seen - told >= REPORT_EVERY {
                told = seen;
                progress(before + seen);
            }
            Ok(())
        },
    )?;
    let digest: String = hash.finalize().iter().map(|b| format!("{b:02x}")).collect();
    if seen != file.bytes || digest != file.sha256 {
        return Err(OnlineError::Protocol {
            provider: PROVIDER.to_string(),
            reason: format!("{} is not the expected file", file.name),
        });
    }
    out.sync_all().map_err(io)
}

/// The text the end-to-end tests read in place of the model.
pub(crate) fn test_text() -> Option<String> {
    if cfg!(feature = "test-harness") {
        // empty: the real model, for the L7 check
        std::env::var("NIKOMIRU_TEST_OCR")
            .ok()
            .filter(|t| !t.is_empty())
    } else {
        None
    }
}

fn recognize(engine: &SharedEngine, job: &OcrJob) -> Result<Recognized, nikomiru_ocr::OcrError> {
    let crop = if let Some(bounds) = job.pixel_bounds {
        nikomiru_ocr::crop_pixels(&job.picture, bounds)?
    } else {
        nikomiru_ocr::crop(&job.picture, job.area)?
    };
    let text = match job.preset_text.clone().or_else(test_text) {
        Some(text) => text,
        None => engine.get()?.read(&crop)?,
    };
    Ok(Recognized {
        text,
        crop: nikomiru_ocr::jpeg(&crop)?,
    })
}

/// The page cache's file for `picture`: the models' revisions in its name,
/// so other models read again.
fn page_cache(cache: &Path, picture: &[u8]) -> PathBuf {
    let digest: String = Sha256::digest(picture)
        .iter()
        .take(16)
        .map(|b| format!("{b:02x}"))
        .collect();
    let models = format!("{}{}", &DETECTOR.sha256[..8], &MODEL[0].sha256[..8]);
    // 5: the lines the detector marks, measured on the picture (MNG-012)
    cache.join(format!("{digest}-{models}-13.json"))
}

#[derive(serde::Serialize, serde::Deserialize)]
struct CachedPage {
    width: u32,
    height: u32,
    regions: Vec<CachedRegion>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct CachedRegion {
    bounds: [u32; 4],
    text: String,
    vertical: bool,
    #[serde(default)]
    lines: Vec<[u32; 4]>,
    #[serde(default)]
    covers: Vec<[u32; 4]>,
    /// Each line's own text, when it was read line by line.
    #[serde(default)]
    parts: Vec<String>,
}

impl CachedPage {
    fn of(page: &Page) -> Self {
        Self {
            width: page.width,
            height: page.height,
            regions: page
                .regions
                .iter()
                .map(|r| CachedRegion {
                    bounds: r.bounds,
                    text: r.text.clone(),
                    vertical: r.vertical,
                    lines: r.lines.iter().map(|(_, b)| *b).collect(),
                    covers: r.covers.clone(),
                    parts: r
                        .lines
                        .iter()
                        .map(|(range, _)| r.text.get(range.clone()).unwrap_or_default().to_string())
                        .collect(),
                })
                .collect(),
        }
    }

    fn page(self) -> Page {
        Page {
            width: self.width,
            height: self.height,
            regions: self
                .regions
                .into_iter()
                .map(|r| {
                    let covers = if r.covers.is_empty() {
                        vec![r.bounds]
                    } else {
                        r.covers
                    };
                    if r.parts.len() == r.lines.len()
                        && !r.parts.is_empty()
                        && r.parts.concat() == r.text
                    {
                        let read = r.parts.into_iter().zip(r.lines).collect();
                        Region::from_lines(r.bounds, r.vertical, read, covers)
                    } else {
                        Region::in_lines(r.bounds, r.text, r.vertical, &r.lines, covers)
                    }
                })
                .collect(),
        }
    }
}

impl Pages {
    fn detector(&self, dir: &Path) -> Result<Arc<Detector>, nikomiru_ocr::OcrError> {
        let mut detector = self.detector.lock().unwrap_or_else(|e| e.into_inner());
        match &*detector {
            Some(loaded) => Ok(loaded.clone()),
            None => Ok(detector.insert(Arc::new(Detector::load(dir)?)).clone()),
        }
    }

    /// The blocks of text on a page, and their text (MNG-011).
    fn read(&self, engine: &SharedEngine, job: &PageJob) -> Result<Page, nikomiru_ocr::OcrError> {
        let cached = self.cache.as_ref().map(|c| page_cache(c, &job.picture));
        if let Some(found) = cached
            .as_ref()
            .and_then(|path| std::fs::read(path).ok())
            .and_then(|bytes| serde_json::from_slice::<CachedPage>(&bytes).ok())
        {
            return Ok(found.page());
        }
        let picture = nikomiru_ocr::decode(&job.picture)?;
        let (width, height) = (picture.width(), picture.height());
        let blocks = match test_text() {
            // the tests' text, in a block in the middle of the page
            Some(_) => vec![nikomiru_ocr::Block {
                lines: Vec::new(),
                bounds: [width / 4, height / 4, width * 3 / 4, height * 3 / 4],
                vertical: true,
            }],
            None => self.detector(&engine.dir)?.find(&picture)?,
        };
        let mut regions = Vec::with_capacity(blocks.len());
        for block in blocks {
            // a box the reader dragged is read first
            while self.asked.load(Ordering::Acquire) > 0 {
                std::thread::sleep(Duration::from_millis(20));
            }
            // its lines and ink as the picture draws them, then each line
            // read, all of them together (MNG-012)
            let drawn =
                nikomiru_ocr::layout(&picture, &block, nikomiru_ocr::estimated_chars(&block));
            match test_text() {
                Some(_) | None if drawn.lines.is_empty() => {
                    // no line stands out: the block read whole
                    let [l, t, r, b] = block.bounds;
                    let text = match test_text() {
                        Some(text) => text,
                        None => engine.get()?.read(&picture.crop_imm(l, t, r - l, b - t))?,
                    };
                    if !text.is_empty() {
                        regions.push(Region::in_lines(
                            block.bounds,
                            text,
                            block.vertical,
                            &[],
                            drawn.covers,
                        ));
                    }
                }
                _ => {
                    let crops: Vec<_> = drawn
                        .lines
                        .iter()
                        .map(|&[l, t, r, b]| {
                            let (l, t) = (l.saturating_sub(2), t.saturating_sub(2));
                            let (r, b) = ((r + 2).min(width), (b + 2).min(height));
                            picture.crop_imm(l, t, r - l, b - t)
                        })
                        .collect();
                    let texts = match test_text() {
                        Some(text) => vec![text; crops.len()],
                        None => engine.get()?.read_many(&crops)?,
                    };
                    let read: Vec<(String, [u32; 4])> = texts
                        .into_iter()
                        .zip(drawn.lines.iter().copied())
                        .filter(|(text, _)| !text.is_empty())
                        .collect();
                    if !read.is_empty() {
                        regions.push(Region::from_lines(
                            block.bounds,
                            block.vertical,
                            read,
                            drawn.covers,
                        ));
                    }
                }
            }
        }
        let page = Page {
            width,
            height,
            regions,
        };
        if let Some(path) = cached
            && let Ok(bytes) = serde_json::to_vec(&CachedPage::of(&page))
        {
            let written = path
                .parent()
                .map_or(Ok(()), std::fs::create_dir_all)
                .and_then(|()| std::fs::write(&path, bytes));
            if let Err(e) = written {
                tracing::warn!("keeping a page read: {e}");
            }
        }
        Ok(page)
    }

    /// Reads the pages queued, one at a time, until none comes for a while.
    fn run(&self, engine: &SharedEngine) {
        loop {
            let next = {
                let mut queue = self.queue.lock().unwrap_or_else(|e| e.into_inner());
                if queue.jobs.is_empty() {
                    queue = self
                        .more
                        .wait_timeout(queue, PAGES_IDLE)
                        .unwrap_or_else(|e| e.into_inner())
                        .0;
                }
                match (queue.jobs.pop_front(), queue.done.clone()) {
                    (Some(job), Some(done)) => Some((job, done)),
                    _ if queue.jobs.is_empty() => {
                        queue.running = false;
                        return;
                    }
                    _ => None,
                }
            };
            if let Some((job, done)) = next {
                let read = self.read(engine, &job).map_err(|e| e.to_string());
                done((job.uid, job.page, read));
            }
        }
    }
}

impl OcrService for OcrWorker {
    fn installed(&self) -> bool {
        self.shared().available()
    }

    fn pages_installed(&self) -> bool {
        test_text().is_some()
            || (nikomiru_ocr::installed(&self.dir) && nikomiru_ocr::detector_installed(&self.dir))
    }

    fn prepare(&self, pages: Vec<PageJob>, done: PageReports) {
        let mut queue = self.pages.queue.lock().unwrap_or_else(|e| e.into_inner());
        queue.jobs = pages.into();
        queue.done = Some(done);
        if !queue.running && !queue.jobs.is_empty() {
            let (pages, engine) = (self.pages.clone(), self.shared());
            let spawned = std::thread::Builder::new()
                .name("ocr-pages".into())
                .spawn(move || pages.run(&engine));
            match spawned {
                Ok(_) => queue.running = true,
                Err(e) => tracing::error!("cannot start the pages' thread: {e}"),
            }
        }
        self.pages.more.notify_all();
    }

    fn install(&self, proxy: String, translator: Translator, reports: Reports) {
        let dir = self.dir.clone();
        let spawned = std::thread::Builder::new()
            .name("ocr-fetch".into())
            .spawn(move || {
                let done =
                    Http::with_timeouts(&proxy, CONNECT_TIMEOUT, FETCH_TIMEOUT).and_then(|http| {
                        install(&http, &FILES, &dir, &|bytes| {
                            reports(OcrEvent::Progress(bytes));
                        })
                    });
                reports(match done {
                    Ok(()) => OcrEvent::Installed,
                    Err(e) => {
                        tracing::warn!("the reading model: {e}");
                        OcrEvent::Failed(translator.error(&e as &dyn Localized))
                    }
                });
            });
        if let Err(e) = spawned {
            tracing::error!("cannot start the model's thread: {e}");
        }
    }

    fn warm(&self) {
        let engine = self.shared();
        if test_text().is_some() || !nikomiru_ocr::installed(&engine.dir) || engine.loaded() {
            return;
        }
        let spawned = std::thread::Builder::new()
            .name("ocr-warm".into())
            .spawn(move || {
                if let Err(e) = engine.get() {
                    tracing::warn!("loading the reading model: {e}");
                }
            });
        if let Err(e) = spawned {
            tracing::error!("cannot start the model's loading: {e}");
        }
    }

    fn read(&self, job: OcrJob, translator: Translator, done: Box<dyn FnOnce(Reading) + Send>) {
        let engine = self.shared();
        let pages = self.pages.clone();
        // reading ahead waits for this one
        pages.asked.fetch_add(1, Ordering::AcqRel);
        let spawned = std::thread::Builder::new()
            .name("ocr".into())
            .spawn(move || {
                let read = recognize(&engine, &job).map_err(|e| {
                    tracing::warn!("reading a page: {e}");
                    translator.error(&e as &dyn Localized)
                });
                pages.asked.fetch_sub(1, Ordering::AcqRel);
                done((job.uid, job.page, read));
            });
        if let Err(e) = spawned {
            self.pages.asked.fetch_sub(1, Ordering::AcqRel);
            tracing::error!("cannot start the reading thread: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    use super::*;

    /// A server that answers every request with `body`, for `count` requests.
    fn serve(body: Vec<u8>, count: usize) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            for stream in listener.incoming().take(count) {
                let mut stream = stream.unwrap();
                let mut request = [0u8; 1024];
                let _ = stream.read(&mut request);
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                stream.write_all(head.as_bytes()).unwrap();
                stream.write_all(&body).unwrap();
            }
        });
        format!("http://{address}/file")
    }

    fn sha256(bytes: &[u8]) -> String {
        Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    // T-MNG-004: a file goes in its place whole, and again it is not fetched
    #[test]
    fn a_model_is_installed_once() {
        let dir = tempfile::tempdir().unwrap();
        let body = vec![7u8; 3 << 20];
        let sum = sha256(&body);
        let url = serve(body.clone(), 1);
        let file = ModelFile {
            name: "encoder.onnx",
            url: &url,
            sha256: &sum,
            bytes: body.len() as u64,
        };
        let http = Http::new("").unwrap();
        let heard = Mutex::new(Vec::new());
        install(&http, &[file], dir.path(), &|n| {
            heard.lock().unwrap().push(n);
        })
        .unwrap();
        assert_eq!(
            std::fs::read(dir.path().join("encoder.onnx")).unwrap(),
            body
        );
        assert!(!dir.path().join("encoder.onnx.part").exists());
        assert!(!heard.lock().unwrap().is_empty(), "progress");
        // the server has gone: only a second fetch would fail
        install(&http, &[file], dir.path(), &|_| {}).unwrap();
    }

    // T-MNG-004: a file that is not the one pinned never takes its name
    #[test]
    fn a_wrong_file_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let url = serve(vec![1, 2, 3], 2);
        let good = sha256(&[9, 9, 9]);
        let file = ModelFile {
            name: "encoder.onnx",
            url: &url,
            sha256: &good,
            bytes: 3,
        };
        let http = Http::new("").unwrap();
        let e = install(&http, &[file], dir.path(), &|_| {}).unwrap_err();
        assert!(matches!(e, OnlineError::Protocol { .. }), "{e:?}");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
        // nor one longer than it should be
        let short = ModelFile { bytes: 2, ..file };
        let e = install(&http, &[short], dir.path(), &|_| {}).unwrap_err();
        assert!(matches!(e, OnlineError::TooLarge { .. }), "{e:?}");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }
    // T-MNG-011: a page read is kept, and comes back with its lines; another
    // picture, or other models, would not find it
    #[test]
    fn a_page_read_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        let path = page_cache(dir.path(), b"page one");
        assert_ne!(path, page_cache(dir.path(), b"page two"));
        assert!(path.to_string_lossy().contains(&format!(
            "{}{}",
            &DETECTOR.sha256[..8],
            &MODEL[0].sha256[..8]
        )));
        let page = Page {
            width: 100,
            height: 200,
            regions: vec![Region::guessed(
                [0, 0, 100, 200],
                "今日は晴れです！".into(),
                true,
            )],
        };
        let bytes = serde_json::to_vec(&CachedPage::of(&page)).unwrap();
        let back = serde_json::from_slice::<CachedPage>(&bytes).unwrap().page();
        assert_eq!(back, page);
    }

    // T-MNG-008: original imported text is used even with no model on disk.
    #[test]
    fn imported_region_prepares_its_crop_without_loading_a_model() {
        let mut bytes = Vec::new();
        image::RgbImage::new(100, 200)
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        let engine = SharedEngine {
            dir: std::path::PathBuf::from("/missing/mokuro-model"),
            engine: Arc::default(),
        };
        let result = recognize(
            &engine,
            &OcrJob {
                uid: "mokuro".into(),
                page: 0,
                picture: bytes,
                area: [0.1, 0.1, 0.5, 0.5],
                preset_text: Some("猫です".into()),
                pixel_bounds: Some([10, 20, 60, 120]),
            },
        )
        .unwrap();
        assert_eq!(result.text, "猫です");
        let image = image::load_from_memory(&result.crop).unwrap();
        assert_eq!((image.width(), image.height()), (50, 100));
        assert!(engine.engine.lock().unwrap().is_none());
    }
}
