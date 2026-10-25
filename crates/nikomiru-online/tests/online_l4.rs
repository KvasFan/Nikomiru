// SPDX-License-Identifier: GPL-3.0-only

//! L4: the Jimaku and OpenSubtitles clients, the search across them and the
//! automatic subtitles against the fake server (T-MET-003, T-MET-004,
//! T-MET-007).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::{Cursor, Write};

use nikomiru_online::fake::{FakeOnline, OsSubtitle};
use nikomiru_online::http::Http;
use nikomiru_online::jimaku::Jimaku;
use nikomiru_online::opensubtitles::OpenSubtitles;
use nikomiru_online::search::{automatic, fetch, search_all};
use nikomiru_online::{FileKind, OnlineError, Provider, ProviderId, SearchRequest, VideoHash};

fn jimaku(fake: &FakeOnline) -> Box<dyn Provider> {
    Box::new(Jimaku::new(Http::new("").unwrap(), &fake.jimaku_url(), "jimaku-key").unwrap())
}

fn opensubtitles(fake: &FakeOnline, user: &str, password: &str) -> Box<dyn Provider> {
    Box::new(
        OpenSubtitles::new(
            Http::new("").unwrap(),
            &fake.opensubtitles_url(),
            "os-key",
            user,
            password,
        )
        .unwrap(),
    )
}

fn request(title: &str, episode: Option<u32>) -> SearchRequest {
    SearchRequest {
        title: title.into(),
        episode,
        language: "ja".into(),
        ..SearchRequest::default()
    }
}

fn zip_of(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, data) in files {
        w.start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        w.write_all(data).unwrap();
    }
    w.finish().unwrap().into_inner()
}

fn frieren(fake: &FakeOnline) {
    let mut d = fake.data();
    d.entries = vec![
        (1, "Sousou no Frieren".into(), true),
        (2, "Frieren Drama".into(), false),
    ];
    d.files.insert(
        1,
        vec![
            ("Sousou no Frieren - 01.ja.srt".into(), b"one".to_vec()),
            ("Sousou no Frieren - 02.srt".into(), b"two".to_vec()),
            ("Sousou no Frieren - 02.zh.srt".into(), b"zh".to_vec()),
            ("Sousou no Frieren S1.rar".into(), b"rar".to_vec()),
            (
                "Sousou no Frieren S1.zip".into(),
                zip_of(&[
                    ("Frieren - 03.ass", b"three"),
                    ("Frieren - 04.ass", b"four"),
                ]),
            ),
        ],
    );
    d.files
        .insert(2, vec![("Frieren Drama 01.srt".into(), b"drama".to_vec())]);
}

// T-MET-003
#[test]
fn jimaku_finds_ranks_and_downloads() {
    let fake = FakeOnline::start().unwrap();
    frieren(&fake);
    let mut providers = vec![jimaku(&fake)];
    let r = request("Sousou no Frieren", Some(2));
    let found = search_all(&mut providers, &r);
    assert!(found.failed.is_empty(), "{:?}", found.failed);
    let names: Vec<&str> = found
        .candidates
        .iter()
        .map(|c| c.file_name.as_str())
        .collect();
    assert_eq!(names[0], "Sousou no Frieren - 02.srt", "{names:?}");
    assert!(names.contains(&"Sousou no Frieren S1.rar"));
    // every request carried the key, only to Jimaku
    for req in fake.requests() {
        assert_eq!(req.header("authorization"), Some("jimaku-key"), "{req:?}");
    }
    let file = fetch(&mut providers, &found.candidates[0], &r).unwrap();
    assert_eq!(file.bytes, b"two");
    // the episode taken out of the season pack
    let pack = found
        .candidates
        .iter()
        .find(|c| c.kind == FileKind::Zip)
        .unwrap();
    let three = fetch(&mut providers, pack, &request("Sousou no Frieren", Some(3))).unwrap();
    assert_eq!(
        (three.name.as_str(), three.bytes.as_slice()),
        ("Frieren - 03.ass", &b"three"[..])
    );
    // a RAR is listed, never downloaded
    let rar = found
        .candidates
        .iter()
        .find(|c| c.kind == FileKind::Unusable)
        .unwrap();
    assert!(matches!(
        fetch(&mut providers, rar, &r),
        Err(OnlineError::Unusable(_))
    ));
}

// T-SET-007: the proxy of the settings carries the requests
#[test]
fn the_proxy_of_the_settings_is_used() {
    let fake = FakeOnline::start().unwrap();
    frieren(&fake);
    // nothing listens on port 1: only the proxy can answer
    let through = Jimaku::new(
        Http::new(&fake.proxy_url()).unwrap(),
        "http://localhost:1/jimaku/api",
        "jimaku-key",
    )
    .unwrap();
    let mut providers: Vec<Box<dyn Provider>> = vec![Box::new(through)];
    let found = search_all(&mut providers, &request("Sousou no Frieren", Some(1)));
    assert!(found.failed.is_empty(), "{:?}", found.failed);
    assert!(!found.candidates.is_empty());
    assert!(
        fake.requests()[0]
            .target
            .starts_with("http://localhost:1/jimaku/api/"),
        "{:?}",
        fake.requests()[0]
    );
    assert!(Http::new("not a proxy").is_err());
}

// T-MET-003: live action is searched when no anime entry matches (BUG-1694)
#[test]
fn jimaku_searches_live_action_after_anime() {
    let fake = FakeOnline::start().unwrap();
    frieren(&fake);
    let mut providers = vec![jimaku(&fake)];
    let found = search_all(&mut providers, &request("Frieren Drama", Some(1)));
    assert_eq!(found.candidates[0].file_name, "Frieren Drama 01.srt");
    let searches: Vec<String> = fake
        .requests()
        .iter()
        .filter(|r| r.target.contains("entries/search"))
        .map(|r| r.target.clone())
        .collect();
    assert_eq!(searches.len(), 2, "{searches:?}");
    assert!(searches[1].contains("anime=false"));
}

// T-MET-003, T-MET-009
#[test]
fn errors_are_named() {
    let fake = FakeOnline::start().unwrap();
    frieren(&fake);
    let mut wrong: Vec<Box<dyn Provider>> = vec![Box::new(
        Jimaku::new(Http::new("").unwrap(), &fake.jimaku_url(), "wrong").unwrap(),
    )];
    let found = search_all(&mut wrong, &request("Frieren", Some(1)));
    assert_eq!(
        found.failed,
        [(
            ProviderId::Jimaku,
            OnlineError::Unauthorized {
                provider: "Jimaku".into()
            }
        )]
    );
    fake.then_status(429, Some(17));
    let found = search_all(&mut [jimaku(&fake)], &request("Frieren", Some(1)));
    assert_eq!(
        found.failed[0].1,
        OnlineError::RateLimited {
            provider: "Jimaku".into(),
            seconds: 17
        }
    );
    // nothing listening
    let dead = Jimaku::new(Http::new("").unwrap(), "http://127.0.0.1:1/api", "k").unwrap();
    let found = search_all(
        &mut [Box::new(dead) as Box<dyn Provider>],
        &request("x", None),
    );
    assert!(matches!(found.failed[0].1, OnlineError::Unreachable { .. }));
}

fn os_subtitles(fake: &FakeOnline) {
    let mut d = fake.data();
    d.os_subtitles = vec![
        OsSubtitle {
            file_id: 7,
            file_name: "Movie.ja.srt".into(),
            language: "ja".into(),
            moviehash: Some("00000000000493e0".into()),
            title: "Movie".into(),
            bytes: b"by hash".to_vec(),
            ..OsSubtitle::default()
        },
        OsSubtitle {
            file_id: 8,
            file_name: "Show.S01E02.srt".into(),
            language: "ja".into(),
            title: "Show".into(),
            season: Some(1),
            episode: Some(2),
            bytes: b"by title".to_vec(),
            ..OsSubtitle::default()
        },
        OsSubtitle {
            file_id: 9,
            file_name: "Show.S01E02.mt.srt".into(),
            language: "ja".into(),
            title: "Show".into(),
            season: Some(1),
            episode: Some(2),
            ai_translated: true,
            bytes: b"machine".to_vec(),
            ..OsSubtitle::default()
        },
    ];
}

// T-MET-004
#[test]
fn opensubtitles_by_hash_then_by_title() {
    let fake = FakeOnline::start().unwrap();
    os_subtitles(&fake);
    let mut providers = vec![opensubtitles(&fake, "", "")];
    let mut r = request("Movie", None);
    r.hash = Some(VideoHash {
        hash: 0x493e0,
        size: 300_000,
    });
    let found = search_all(&mut providers, &r);
    assert_eq!(found.candidates.len(), 1);
    assert_eq!(found.candidates[0].file_name, "Movie.ja.srt");
    let target = &fake.requests()[0].target;
    assert!(
        target.contains("languages=ja&moviebytesize=300000&moviehash=00000000000493e0"),
        "sorted parameters: {target}"
    );
    // a hash that finds nothing: the title, season and episode
    fake.clear_requests();
    let mut r = request("Show", Some(2));
    r.season = Some(1);
    r.hash = Some(VideoHash { hash: 1, size: 2 });
    let found = search_all(&mut providers, &r);
    assert_eq!(fake.requests().len(), 2);
    assert_eq!(found.candidates[0].file_name, "Show.S01E02.srt");
    assert!(found.candidates[1].machine_translated);
    // the download: a temporary link fetched without the key
    fake.clear_requests();
    let file = fetch(&mut providers, &found.candidates[0], &r).unwrap();
    assert_eq!(file.bytes, b"by title");
    let requests = fake.requests();
    assert_eq!(requests[0].body, r#"{"file_id":8}"#);
    assert_eq!(requests[1].target, "/os/files/8");
    assert_eq!(requests[1].header("api-key"), None);
    assert_eq!(fake.data().remaining, 9);
}

// T-MET-004: the login for the quota, and the quota used up
#[test]
fn opensubtitles_logs_in_and_reports_the_quota() {
    let fake = FakeOnline::start().unwrap();
    os_subtitles(&fake);
    fake.data().os_login = Some(("me".into(), "pw".into()));
    let r = request("Show", Some(2));
    let mut wrong = vec![opensubtitles(&fake, "me", "nope")];
    let c = search_all(&mut wrong, &r).candidates[0].clone();
    assert_eq!(
        fetch(&mut wrong, &c, &r),
        Err(OnlineError::Unauthorized {
            provider: "OpenSubtitles".into()
        })
    );
    let mut right = vec![opensubtitles(&fake, "me", "pw")];
    assert_eq!(fetch(&mut right, &c, &r).unwrap().bytes, b"by title");
    let login = fake
        .requests()
        .into_iter()
        .rfind(|q| q.target.ends_with("/login"))
        .unwrap();
    assert!(login.body.contains("\"username\":\"me\""));
    fake.data().remaining = 0;
    assert_eq!(
        fetch(&mut right, &c, &r),
        Err(OnlineError::Quota {
            provider: "OpenSubtitles".into()
        })
    );
}

// T-MET-007
#[test]
fn automatic_takes_the_first_file_that_fits() {
    let fake = FakeOnline::start().unwrap();
    frieren(&fake);
    os_subtitles(&fake);
    let mut providers = vec![jimaku(&fake), opensubtitles(&fake, "", "")];
    // episode 4: only in the season pack
    let (c, file) = automatic(&mut providers, &request("Sousou no Frieren", Some(4)))
        .unwrap()
        .unwrap();
    assert_eq!(c.kind, FileKind::Zip);
    assert_eq!(file.bytes, b"four");
    // the machine translation is never taken
    let (c, _) = automatic(&mut providers, &request("Show", Some(2)))
        .unwrap()
        .unwrap();
    assert!(!c.machine_translated);
    // nothing for episode 9
    assert_eq!(
        automatic(&mut providers, &request("Sousou no Frieren", Some(9))).unwrap(),
        None
    );
    // every provider failed: the error
    let mut wrong: Vec<Box<dyn Provider>> = vec![Box::new(
        Jimaku::new(Http::new("").unwrap(), &fake.jimaku_url(), "wrong").unwrap(),
    )];
    assert!(automatic(&mut wrong, &request("Frieren", Some(1))).is_err());
}
