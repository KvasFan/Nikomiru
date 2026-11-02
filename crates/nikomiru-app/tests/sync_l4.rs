// SPDX-License-Identifier: GPL-3.0-only

//! L4: the `sync` worker against the fake WebDAV, with two devices on one
//! computer (T-REV-006).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc;
use std::time::{Duration, SystemTime};

use nikomiru_app::sync::{Parts, SyncWorker};
use nikomiru_core::SystemClock;
use nikomiru_core::config::SyncConfig;
use nikomiru_library::Library;
use nikomiru_library::review::{Memory, ReviewCard};
use nikomiru_online::fake_servers::{Data, FakeServers};
use nikomiru_ui::i18n::Translator;
use nikomiru_ui::sync::SyncService;

struct Device {
    library: Arc<Library>,
    review: PathBuf,
    worker: SyncWorker,
}

fn device(dir: &Path, id: &str, url: &str) -> Device {
    let data = dir.join(id);
    std::fs::create_dir_all(&data).unwrap();
    let library = Arc::new(Library::open(&data.join("library.redb")).unwrap());
    let worker = SyncWorker::start(Parts {
        sync: SyncConfig {
            url: url.into(),
            user: "dav".into(),
            password: "pass".into(),
            ..SyncConfig::default()
        },
        proxy: String::new(),
        device_id: id.into(),
        library: Some(Arc::clone(&library)),
        data_dir: data.clone(),
        presets_dir: data.join("mpv"),
        translator: Translator::from_setting("en", |_| None),
        clock: Arc::new(SystemClock::new()),
    });
    Device {
        library,
        review: data.join("review"),
        worker,
    }
}

fn sync(d: &Device) {
    let (tx, rx) = mpsc::channel();
    d.worker.sync(Box::new(move |r| tx.send(r).unwrap()));
    rx.recv_timeout(Duration::from_secs(30)).unwrap().unwrap();
}

fn card(audio: &str, picture: &str) -> ReviewCard {
    ReviewCard {
        expression: "食べる".into(),
        reading: "たべる".into(),
        sentence: "食べました".into(),
        word: None,
        glossaries: vec![("JMdict".into(), "to eat".into())],
        audio: Some(audio.into()),
        picture: Some(picture.into()),
        video_uid: "video/av_basic".into(),
        title: "av_basic".into(),
        start_ms: 0,
        end_ms: 1_000,
        created_at: 1_000,
        memory: Memory::default(),
    }
}

// T-REV-006: a card's files go up with it and come down on the other device;
// a file whose content lies is refused; a deletion takes them from both
#[test]
fn the_reviews_media_follow_its_cards() {
    let fake = FakeServers::start(Data::default()).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let a = device(dir.path(), "one", &fake.webdav_url());
    let b = device(dir.path(), "two", &fake.webdav_url());
    let audio_bytes = b"sound of the line".to_vec();
    let picture_bytes = b"frame of the scene".to_vec();
    let audio = nikomiru_anki::media::name(&audio_bytes, "m4a");
    let picture = nikomiru_anki::media::name(&picture_bytes, "jpg");
    std::fs::create_dir_all(&a.review).unwrap();
    std::fs::write(a.review.join(&audio), &audio_bytes).unwrap();
    std::fs::write(a.review.join(&picture), &picture_bytes).unwrap();
    let id = a.library.add_review_card(&card(&audio, &picture)).unwrap();
    sync(&a);
    {
        let server = fake.data();
        assert_eq!(
            server.dav_files[&format!("nikomiru/review/{audio}")],
            audio_bytes
        );
        assert!(server.dav_files.contains_key("nikomiru/devices/one.sync"));
    }
    // the picture on the server is not what its name says
    fake.data().dav_files.insert(
        format!("nikomiru/review/{picture}"),
        b"something else".to_vec(),
    );
    sync(&b);
    assert_eq!(
        b.library.review_card(id).unwrap(),
        Some(card(&audio, &picture))
    );
    assert_eq!(std::fs::read(b.review.join(&audio)).unwrap(), audio_bytes);
    assert!(!b.review.join(&picture).exists(), "refused");
    // deleted on b: gone from a, and so are its files, once old enough
    b.library.delete_review_card(id).unwrap();
    sync(&b);
    let long_ago = SystemTime::now() - Duration::from_secs(3600);
    for name in [&audio, &picture] {
        let file = std::fs::File::options()
            .write(true)
            .open(a.review.join(name))
            .unwrap();
        file.set_modified(long_ago).unwrap();
    }
    sync(&a);
    assert_eq!(a.library.review_card(id).unwrap(), None);
    assert!(!a.review.join(&audio).exists() && !a.review.join(&picture).exists());
}
