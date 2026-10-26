// SPDX-License-Identifier: GPL-3.0-only

//! L4: series metadata from AniList and Jikan against the fake server, and
//! Jimaku asked by the AniList id (T-MET-001, T-MET-002, T-MET-003).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use nikomiru_core::ManualClock;
use nikomiru_online::fake::{FakeOnline, FakeSeries};
use nikomiru_online::http::Http;
use nikomiru_online::jimaku::Jimaku;
use nikomiru_online::metadata::{AniList, Identified, Jikan, Limiter, MetadataSource, identify};
use nikomiru_online::{OnlineError, Provider, SearchRequest};

fn series(fake: &FakeOnline) {
    let mut d = fake.data();
    d.series = vec![
        FakeSeries {
            anilist_id: 154_587,
            mal_id: 52_991,
            romaji: "Sousou no Frieren".into(),
            english: Some("Frieren: Beyond Journey's End".into()),
            native: Some("葬送のフリーレン".into()),
            episodes: Some(28),
        },
        FakeSeries {
            anilist_id: 182_255,
            mal_id: 59_978,
            romaji: "Sousou no Frieren 2nd Season".into(),
            english: None,
            native: Some("葬送のフリーレン 第2期".into()),
            episodes: None,
        },
    ];
}

// T-MET-001
#[test]
fn anilist_identifies_an_exact_unique_title_and_lets_the_user_choose_otherwise() {
    let fake = FakeOnline::start().unwrap();
    series(&fake);
    let mut anilist = AniList::new(Http::new("").unwrap(), &fake.anilist_url()).unwrap();
    let found = anilist.search("Sousou no Frieren").unwrap();
    assert_eq!(found.len(), 2);
    let Identified::Unique(frieren) = identify("Sousou no Frieren", found) else {
        panic!("not identified");
    };
    assert_eq!(
        (frieren.anilist_id, frieren.mal_id, frieren.episodes),
        (Some(154_587), Some(52_991), Some(28))
    );
    assert!(frieren.cover_url.unwrap().ends_with("/covers/154587.jpg"));
    // a title that is not exact: the user chooses
    let found = anilist.search("Frieren").unwrap();
    assert!(matches!(identify("Frieren", found), Identified::Choose(c) if c.len() == 2));
    assert_eq!(
        identify("x", anilist.search("Nothing like it").unwrap()),
        Identified::Nothing
    );
    // the same title again comes from the cache
    let before = fake.requests().len();
    anilist.search("sousou no frieren").unwrap();
    assert_eq!(fake.requests().len(), before);
}

// T-MET-002
#[test]
fn jikan_asks_once_a_second_pauses_after_429_and_keeps_what_it_found() {
    let fake = FakeOnline::start().unwrap();
    series(&fake);
    let clock = Arc::new(ManualClock::new(SystemTime::UNIX_EPOCH));
    let slept = Arc::new(Mutex::new(Vec::new()));
    let limiter = {
        let (clock, slept) = (clock.clone(), slept.clone());
        Limiter::new(
            clock.clone(),
            Box::new(move |d| {
                slept.lock().unwrap().push(d);
                clock.advance(d);
            }),
        )
    };
    let mut jikan = Jikan::new(Http::new("").unwrap(), &fake.jikan_url(), limiter).unwrap();
    let found = jikan.search("Sousou no Frieren").unwrap();
    assert_eq!(found[0].mal_id, Some(52_991));
    assert_eq!(found[0].anilist_id, None);
    jikan.search("Frieren").unwrap();
    jikan.search("葬送のフリーレン").unwrap();
    // three requests: two waits of a second between them
    assert_eq!(
        *slept.lock().unwrap(),
        [Duration::from_secs(1), Duration::from_secs(1)]
    );
    // the cache answers at once
    let before = fake.requests().len();
    jikan.search("sousou no FRIEREN").unwrap();
    assert_eq!(fake.requests().len(), before);
    // a 429 pauses it: no request until the pause ends
    fake.then_status(429, Some(30));
    let e = jikan.search("Dandadan").unwrap_err();
    assert!(
        matches!(e, OnlineError::RateLimited { seconds: 30, .. }),
        "{e:?}"
    );
    let asked = fake.requests().len();
    let e = jikan.search("Dandadan").unwrap_err();
    assert!(matches!(e, OnlineError::RateLimited { .. }), "{e:?}");
    assert_eq!(
        fake.requests().len(),
        asked,
        "nothing asked during the pause"
    );
    clock.advance(Duration::from_secs(31));
    assert!(jikan.search("Dandadan").unwrap().is_empty());
    assert_eq!(fake.requests().len(), asked + 1);
}

// T-MET-003: Jimaku is asked by the series' AniList id first
#[test]
fn jimaku_finds_an_identified_series_by_its_anilist_id() {
    let fake = FakeOnline::start().unwrap();
    {
        let mut d = fake.data();
        d.entries = vec![(7, "Frieren".into(), true)];
        d.entry_anilist.insert(7, 154_587);
        d.files
            .insert(7, vec![("Frieren.S01E03.ja.srt".into(), b"1".to_vec())]);
    }
    let mut jimaku = Jimaku::new(Http::new("").unwrap(), &fake.jimaku_url(), "jimaku-key").unwrap();
    // a title Jimaku would not find
    let request = SearchRequest {
        title: "葬送のフリーレン".into(),
        episode: Some(3),
        language: "ja".into(),
        anilist_id: Some(154_587),
        ..SearchRequest::default()
    };
    let found = jimaku.search(&request).unwrap();
    assert_eq!(found.len(), 1);
    assert!(
        fake.requests()
            .iter()
            .any(|r| r.target.contains("anilist_id=154587")),
        "{:?}",
        fake.requests()
    );
    // not identified: by title only
    fake.clear_requests();
    let request = SearchRequest {
        anilist_id: None,
        ..request
    };
    assert!(jimaku.search(&request).unwrap().is_empty());
    assert!(
        !fake
            .requests()
            .iter()
            .any(|r| r.target.contains("anilist_id"))
    );
}

// T-MET-LIVE: the real AniList and Jikan (L7, run by hand: `cargo test -p
// nikomiru-online --features test-support --test metadata_l4 -- --ignored l7_`)
#[test]
#[ignore = "L7: the real AniList and Jikan, on the network"]
fn l7_anilist_and_jikan_identify_a_real_series() {
    let http = Http::new("").unwrap();
    let mut anilist = AniList::new(http.clone(), nikomiru_online::metadata::ANILIST).unwrap();
    let found = anilist.search("Sousou no Frieren").unwrap();
    let Identified::Unique(f) = identify("Sousou no Frieren", found) else {
        panic!("AniList did not identify Sousou no Frieren");
    };
    assert_eq!(f.anilist_id, Some(154_587));
    assert!(f.cover_url.is_some_and(|u| u.starts_with("https://")));
    let mut jikan = Jikan::new(http, nikomiru_online::metadata::JIKAN, Limiter::real()).unwrap();
    let found = jikan.search("Sousou no Frieren").unwrap();
    assert!(found.iter().any(|s| s.mal_id == Some(52_991)), "{found:?}");
}
