// SPDX-License-Identifier: GPL-3.0-only

//! The reader of a manga's pages (MNG-003, MNG-004): the model, fetched once
//! on request and checked before it goes in its place, then loaded the first
//! time a page is read and kept for the next.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use nikomiru_core::Localized;
use nikomiru_ocr::{Engine, MODEL, ModelFile};
use nikomiru_online::error::OnlineError;
use nikomiru_online::http::{CONNECT_TIMEOUT, Http, checked_url};
use nikomiru_ui::i18n::Translator;
use nikomiru_ui::ocr::{OcrEvent, OcrJob, OcrService, Reading, Recognized, Reports};
use sha2::{Digest, Sha256};

/// Who serves the model, for the errors.
const PROVIDER: &str = "Hugging Face";
/// Time for the whole model on a slow line.
const FETCH_TIMEOUT: Duration = Duration::from_secs(3 * 3600);
/// How often the progress is told.
const REPORT_EVERY: u64 = 2 << 20;

/// The worker.
#[derive(Debug)]
pub struct OcrWorker {
    dir: PathBuf,
    engine: Arc<Mutex<Option<Arc<Engine>>>>,
}

impl OcrWorker {
    /// A worker for the model in `dir`.
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            engine: Arc::default(),
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
fn test_text() -> Option<String> {
    if cfg!(feature = "test-harness") {
        std::env::var("NIKOMIRU_TEST_OCR").ok()
    } else {
        None
    }
}

fn recognize(
    engine: &Mutex<Option<Arc<Engine>>>,
    dir: &Path,
    job: &OcrJob,
) -> Result<Recognized, nikomiru_ocr::OcrError> {
    let crop = nikomiru_ocr::crop(&job.picture, job.area)?;
    let text = match test_text() {
        Some(text) => text,
        None => {
            let loaded = {
                let mut engine = engine.lock().unwrap_or_else(|e| e.into_inner());
                match &*engine {
                    Some(loaded) => loaded.clone(),
                    None => engine.insert(Arc::new(Engine::load(dir)?)).clone(),
                }
            };
            loaded.read(&crop)?
        }
    };
    Ok(Recognized {
        text,
        crop: nikomiru_ocr::jpeg(&crop)?,
    })
}

impl OcrService for OcrWorker {
    fn installed(&self) -> bool {
        test_text().is_some() || nikomiru_ocr::installed(&self.dir)
    }

    fn install(&self, proxy: String, translator: Translator, reports: Reports) {
        let dir = self.dir.clone();
        let spawned = std::thread::Builder::new()
            .name("ocr-fetch".into())
            .spawn(move || {
                let done =
                    Http::with_timeouts(&proxy, CONNECT_TIMEOUT, FETCH_TIMEOUT).and_then(|http| {
                        install(&http, &MODEL, &dir, &|bytes| {
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

    fn read(&self, job: OcrJob, translator: Translator, done: Box<dyn FnOnce(Reading) + Send>) {
        let (engine, dir) = (self.engine.clone(), self.dir.clone());
        let spawned = std::thread::Builder::new()
            .name("ocr".into())
            .spawn(move || {
                let read = recognize(&engine, &dir, &job).map_err(|e| {
                    tracing::warn!("reading a page: {e}");
                    translator.error(&e as &dyn Localized)
                });
                done((job.uid, job.page, read));
            });
        if let Err(e) = spawned {
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
}
