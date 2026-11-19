// SPDX-License-Identifier: GPL-3.0-only

//! T-SUB-063: the loader applies the lines edited by hand, kept in the
//! library under a key made from the lines as read, wherever the file is.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::{Arc, mpsc};

use nikomiru_app::subtitles::{Loader, edit_key, read_file};
use nikomiru_library::Library;
use nikomiru_library::subtitle_edits::LineEdit;
use nikomiru_ui::i18n::Translator;
use nikomiru_ui::session::SubtitleSource;
use nikomiru_ui::subtitles::{Loaded, Request, Source};

const SRT: &str = "1\n00:00:01,000 --> 00:00:02,000\n一\n\n2\n00:00:03,000 --> 00:00:04,000\n二\n";

fn tr() -> Translator {
    Translator::from_setting("en", |_| None)
}

fn load(loader: &Loader, path: &std::path::Path) -> Loaded {
    let (tx, rx) = mpsc::channel();
    loader.load(
        Request {
            position_ms: 0,
            preferred_languages: Vec::new(),
            video: path.with_extension("mkv"),
            source: Source::File(path.to_path_buf()),
            encoding: None,
            duration_ms: 10_000,
            secondary: false,
        },
        Box::new(move |r| {
            let _ = tx.send(r);
        }),
    );
    rx.recv_timeout(std::time::Duration::from_secs(20))
        .unwrap()
        .unwrap()
}

#[test]
fn the_loader_applies_the_lines_edited_by_hand() {
    let dir = tempfile::tempdir().unwrap();
    let library = Arc::new(Library::open(&dir.path().join("library.redb")).unwrap());
    let file = dir.path().join("Ep01.ja.srt");
    std::fs::write(&file, SRT).unwrap();
    let key = edit_key(&read_file(&file, None, &tr()).unwrap().track.cues);
    let loader = Loader::start(
        tr(),
        String::new(),
        dir.path().join("cache"),
        nikomiru_app::ocr::OcrWorker::new(dir.path().join("model")).shared(),
        Some(Arc::clone(&library)),
    );
    // nothing edited yet: as in the file, with its key
    let loaded = load(&loader, &file);
    assert_eq!(loaded.edit_key.as_deref(), Some(key.as_str()));
    assert_eq!(loaded.track.cues[1].plain_text, "二");
    let order = loaded.track.cues[1].order as u32;
    let edit = LineEdit {
        start_ms: 3_000,
        end_ms: 4_500,
        text: "二つ".into(),
    };
    assert!(library.set_subtitle_edit(&key, order, Some(edit)).unwrap());
    // the same lines in another folder, under another name
    let moved = dir.path().join("elsewhere.srt");
    std::fs::write(&moved, SRT).unwrap();
    for path in [&file, &moved] {
        let loaded = load(&loader, path);
        assert_eq!(loaded.track.cues[1].plain_text, "二つ");
        assert_eq!(loaded.track.cues[1].end_ms, 4_500);
        assert_eq!(
            loaded.edit_key.as_deref(),
            Some(key.as_str()),
            "the key of the file's lines"
        );
    }
    // the file changed: its edits no longer apply
    std::fs::write(&file, SRT.replace('二', "三")).unwrap();
    let loaded = load(&loader, &file);
    assert_eq!(loaded.track.cues[1].plain_text, "三");
    assert_ne!(loaded.edit_key.as_deref(), Some(key.as_str()));
}
