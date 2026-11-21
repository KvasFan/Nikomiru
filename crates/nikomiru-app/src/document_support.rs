// SPDX-License-Identifier: GPL-3.0-only

//! Native legacy decoding and bounded, explicit Poppler text extraction (DOC-002/004).
use nikomiru_books::{BookError, documents::Support};
use nikomiru_core::Cancel;
use std::{
    ffi::OsString,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

/// Shared format support used by the reader and whole-book analysis workers.
#[derive(Debug, Default)]
pub struct NativeSupport {
    pdf_path: RwLock<String>,
}
impl NativeSupport {
    /// Capture the optional explicit Poppler executable or directory; empty uses environment/PATH.
    pub fn new(pdf_path: String) -> Self {
        Self {
            pdf_path: RwLock::new(pdf_path),
        }
    }
    fn program(&self) -> Option<PathBuf> {
        let configured = self
            .pdf_path
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        find_pdf(&configured)
    }
}
/// Discover an explicit or PATH Poppler executable without downloading or elevating.
pub fn find_pdf(configured: &str) -> Option<PathBuf> {
    let exe = if cfg!(windows) {
        "pdftotext.exe"
    } else {
        "pdftotext"
    };
    let configured = if configured.is_empty() {
        std::env::var_os("NIKOMIRU_PDFTOTEXT").unwrap_or_default()
    } else {
        OsString::from(configured)
    };
    if !configured.is_empty() {
        let mut p = PathBuf::from(configured);
        if p.is_dir() {
            p = p.join(exe);
        }
        return usable(&p);
    }
    std::env::split_paths(&std::env::var_os("PATH")?).find_map(|p| usable(&p.join(exe)))
}
fn usable(p: &Path) -> Option<PathBuf> {
    let meta = std::fs::metadata(p).ok()?;
    if !meta.is_file() {
        return None;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o111 == 0 {
            return None;
        }
    }
    std::fs::canonicalize(p).ok()
}
impl Support for NativeSupport {
    fn decode(&self, bytes: &[u8], encoding: &str) -> Option<String> {
        if encoding == "CP65001" {
            return String::from_utf8(bytes.to_vec()).ok();
        }
        nikomiru_subs::encoding::Decoder::decode(&crate::platform::Iconv, bytes, encoding)
    }
    fn configure_pdf(&self, path: String) {
        *self
            .pdf_path
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = path;
    }
    fn pdf_text(&self, bytes: &[u8], cancel: &Cancel) -> Result<String, BookError> {
        let program = self.program().ok_or(BookError::PdfUnavailable)?;
        validate_pdf(&program, cancel)?;
        let (out, _) = capture(
            &program,
            &["-enc", "UTF-8", "-layout", "-", "-"],
            bytes,
            cancel,
            Duration::from_secs(30),
            4 << 20,
            64 << 10,
        )?;
        String::from_utf8(out).map_err(|_| BookError::InvalidDocument)
    }
}
/// Validate a discovered/helper-install result before accepting its executable path.
pub fn validate_pdf(program: &Path, cancel: &Cancel) -> Result<(), BookError> {
    let (out, err) = capture(
        program,
        &["-v"],
        &[],
        cancel,
        Duration::from_secs(5),
        4096,
        4096,
    )?;
    if ![out, err]
        .iter()
        .any(|b| String::from_utf8_lossy(b).contains("pdftotext version "))
    {
        return Err(BookError::PdfUnavailable);
    }
    Ok(())
}
fn capped(mut r: impl Read, max: usize, over: &AtomicBool) -> std::io::Result<Vec<u8>> {
    let mut out = Vec::new();
    (&mut r).take(max as u64 + 1).read_to_end(&mut out)?;
    if out.len() > max {
        over.store(true, Ordering::Release);
        out.truncate(max);
    }
    Ok(out)
}
fn tool_error(reason: impl ToString) -> BookError {
    BookError::DocumentTool(reason.to_string())
}
fn capture(
    program: &Path,
    args: &[&str],
    input: &[u8],
    cancel: &Cancel,
    timeout: Duration,
    out_limit: usize,
    err_limit: usize,
) -> Result<(Vec<u8>, Vec<u8>), BookError> {
    if cancel.is_cancelled() {
        return Err(tool_error("cancelled"));
    }
    let mut command = Command::new(program);
    command
        .args(args)
        .env("LC_ALL", "C")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let mut tries = 0;
    let mut child = loop {
        match command.spawn() {
            Ok(c) => break c,
            Err(e) if e.kind() == std::io::ErrorKind::ExecutableFileBusy && tries < 20 => {
                tries += 1;
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(e) => return Err(tool_error(e)),
        }
    };
    let stdin = child.stdin.take();
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    // Own the input and flags so a broken wrapper cannot keep a scoped thread
    // join blocked beyond the extraction deadline through an inherited pipe.
    let input = input.to_vec();
    let over = Arc::new(AtomicBool::new(false));
    let writer = std::thread::spawn(move || match stdin {
        Some(mut w) => w.write_all(&input),
        None => Err(std::io::Error::other("stdin unavailable")),
    });
    let out_over = Arc::clone(&over);
    let out =
        std::thread::spawn(move || stdout.map(|r| capped(r, out_limit, &out_over)).transpose());
    let err_over = Arc::clone(&over);
    let err =
        std::thread::spawn(move || stderr.map(|r| capped(r, err_limit, &err_over)).transpose());
    let deadline = Instant::now() + timeout;
    let mut status = None;
    let result = loop {
        if over.load(Ordering::Acquire) {
            break Err(BookError::TooLarge);
        }
        if cancel.is_cancelled() {
            break Err(tool_error("cancelled"));
        }
        if Instant::now() >= deadline {
            break Err(tool_error("extraction timed out"));
        }
        if status.is_none() {
            match child.try_wait() {
                Ok(s) => status = s,
                Err(e) => break Err(tool_error(e)),
            }
        }
        if let Some(status) = status
            && writer.is_finished()
            && out.is_finished()
            && err.is_finished()
        {
            break Ok(status);
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let status = match result {
        Ok(status) => status,
        Err(e) => {
            #[cfg(any(target_os = "linux", target_os = "android"))]
            crate::platform::kill_process_group(child.id());
            let _ = child.kill();
            let _ = child.wait();
            return Err(e);
        }
    };
    let written = writer
        .join()
        .map_err(|_| tool_error("stdin worker failed"))?;
    let out = out
        .join()
        .map_err(|_| tool_error("stdout worker failed"))?
        .map_err(tool_error)?
        .unwrap_or_default();
    let err = err
        .join()
        .map_err(|_| tool_error("stderr worker failed"))?
        .map_err(tool_error)?
        .unwrap_or_default();
    if over.load(Ordering::Acquire) {
        return Err(BookError::TooLarge);
    }
    if !status.success() {
        return Err(tool_error(
            String::from_utf8_lossy(&err)
                .chars()
                .take(4096)
                .collect::<String>(),
        ));
    }
    written.map_err(tool_error)?;
    Ok((out, err))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pdf(text: bool) -> Vec<u8> {
        if text {
            include_bytes!("../../../tests/fixtures/documents/text-pages.pdf").to_vec()
        } else {
            include_bytes!("../../../tests/fixtures/documents/no-text.pdf").to_vec()
        }
    }
    // T-DOC-002: the same native encoding adapter already used for captions handles declared Japanese/Western RTF runs.
    #[test]
    fn native_document_legacy_decoding_is_strict_and_does_not_guess() {
        let s = NativeSupport::default();
        assert_eq!(s.decode(&[0x82, 0xa0], "CP932").as_deref(), Some("あ"));
        assert_eq!(s.decode(&[0xe9], "CP1252").as_deref(), Some("é"));
        assert!(s.decode(&[0x82], "CP932").is_none());
    }
    // T-DOC-004, T-DOC-005: actual Poppler, Unicode ToUnicode maps, empty middle pages, strict no-text/error handling and shared analysis identity.
    #[test]
    #[ignore = "L4: requires installed Poppler pdftotext"]
    fn actual_poppler_extracts_original_japanese_pdf_pages_without_ocr() {
        let program = find_pdf("").expect("install Poppler to run this native check");
        let support = NativeSupport::new(program.to_string_lossy().into_owned());
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Original.pdf");
        std::fs::write(&path, pdf(true)).unwrap();
        let book = nikomiru_books::read_with_support(&path, &support, &Cancel::new()).unwrap();
        assert_eq!(book.chapters.len(), 3);
        assert_eq!(book.chapters[0].paragraphs[0].text.trim(), "猫です。");
        assert!(book.chapters[1].paragraphs.is_empty());
        assert_eq!(book.chapters[2].paragraphs[0].text.trim(), "猫です。");
        let analysis =
            nikomiru_books::analysis::read_text_with_support(&path, &Cancel::new(), &support)
                .unwrap();
        assert_eq!(book.identity, analysis.identity);
        assert_eq!(book.chapters, analysis.chapters);
        std::fs::write(&path, pdf(false)).unwrap();
        assert!(matches!(
            nikomiru_books::read_with_support(&path, &support, &Cancel::new()),
            Err(BookError::NoText)
        ));
        assert!(
            support
                .pdf_text(b"%PDF-1.7 invalid content", &Cancel::new())
                .is_err()
        );
        assert!(matches!(
            support.pdf_text(include_bytes!("../../../tests/fixtures/documents/password.pdf"), &Cancel::new()),
            Err(BookError::DocumentTool(reason)) if reason.to_ascii_lowercase().contains("password")
        ));
        support.configure_pdf(
            dir.path()
                .join("missing helper")
                .to_string_lossy()
                .into_owned(),
        );
        assert!(matches!(
            support.pdf_text(&pdf(true), &Cancel::new()),
            Err(BookError::PdfUnavailable)
        ));
    }
    #[cfg(unix)]
    fn script(dir: &Path, name: &str, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        path
    }
    // T-DOC-004: fake executables exercise real pipes, bounded stdout/stderr, physical timeout and in-flight cancellation.
    #[cfg(unix)]
    #[test]
    fn pdf_helper_pipes_are_bounded_cancelled_and_never_interpreted_by_a_shell() {
        let dir = tempfile::tempdir().unwrap();
        let out = script(dir.path(), "helper $(must-never-run)", "exec yes output");
        assert!(matches!(
            capture(
                &out,
                &[],
                b"",
                &Cancel::new(),
                Duration::from_secs(2),
                1024,
                1024
            ),
            Err(BookError::TooLarge)
        ));
        let err = script(dir.path(), "stderr-helper", "exec yes error >&2");
        assert!(matches!(
            capture(
                &err,
                &[],
                b"",
                &Cancel::new(),
                Duration::from_secs(2),
                1024,
                1024
            ),
            Err(BookError::TooLarge)
        ));
        let asleep = script(dir.path(), "sleep-helper", "exec sleep 10");
        let at = Instant::now();
        assert!(
            matches!(capture(&asleep,&[],b"",&Cancel::new(),Duration::from_millis(80),1024,1024),Err(BookError::DocumentTool(e)) if e.contains("timed out"))
        );
        assert!(at.elapsed() < Duration::from_secs(2));
        // A wrapper may leave descendants holding stdout after its leader exits.
        // Neither that pipe nor a blocked stdin writer may bypass the deadline.
        let descendant = script(dir.path(), "wrapper-helper", "sleep 10 & exit 0");
        let at = Instant::now();
        assert!(matches!(
            capture(&descendant, &[], &vec![b'x'; 128 << 10], &Cancel::new(), Duration::from_millis(80), 1024, 1024),
            Err(BookError::DocumentTool(e)) if e.contains("timed out")
        ));
        assert!(at.elapsed() < Duration::from_secs(2));
        let cancel = Cancel::new();
        let signal = cancel.clone();
        std::thread::scope(|s| {
            s.spawn(move || {
                std::thread::sleep(Duration::from_millis(30));
                signal.cancel();
            });
            assert!(
                matches!(capture(&asleep,&[],b"",&cancel,Duration::from_secs(2),1024,1024),Err(BookError::DocumentTool(e)) if e.contains("cancelled"))
            );
        });
        let wrong = script(dir.path(), "wrong-helper", "echo wrong-tool");
        assert!(validate_pdf(&wrong, &Cancel::new()).is_err());
        let invalid = script(
            dir.path(),
            "invalid-utf8",
            "if [ \"$1\" = -v ]; then echo 'pdftotext version fake' >&2; else cat >/dev/null; printf '\\377'; fi",
        );
        assert!(matches!(
            NativeSupport::new(invalid.to_string_lossy().into_owned())
                .pdf_text(&pdf(true), &Cancel::new()),
            Err(BookError::InvalidDocument)
        ));
    }
}
