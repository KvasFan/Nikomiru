// SPDX-License-Identifier: GPL-3.0-only

//! L2: what a media server answers is untrusted (T-SRV-013): any JSON, or
//! none, gives an error or a page, and never a panic.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use nikomiru_online::http::Http;
use nikomiru_online::servers::jellyfin::Jellyfin;
use nikomiru_online::servers::plex::Plex;
use nikomiru_online::servers::{Item, ItemKind, MediaServer, ParentKind, Url, next_of, parse_time};
use proptest::prelude::*;
use serde_json::Value;

use std::io::{Read, Write};
use std::net::TcpListener;

/// A server that answers `body` once to each of `count` requests.
fn serve(body: Vec<u8>, count: usize) -> String {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for connection in listener.incoming().take(count) {
            let Ok(mut connection) = connection else {
                return;
            };
            let mut head = [0u8; 4096];
            let _ = connection.read(&mut head);
            let line = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = connection.write_all(line.as_bytes());
            let _ = connection.write_all(&body);
        }
    });
    format!("http://127.0.0.1:{port}/")
}

fn exercise(server: &mut dyn MediaServer) {
    let _ = server.libraries();
    let _ = server.children("a", 0, ParentKind::Library);
    let _ = server.children("a", 0, ParentKind::Item);
    let _ = server.search("a", "q", 0);
    let _ = server.item("a");
    let _ = server.playable("a");
}

const CALLS: usize = 16;

fn both(body: Vec<u8>) {
    let http = Http::new("").unwrap();
    let url = Url::parse(&serve(body.clone(), CALLS)).unwrap();
    exercise(&mut Jellyfin::new(http.clone(), url, "t", "u", "d"));
    let url = Url::parse(&serve(body, CALLS)).unwrap();
    exercise(&mut Plex::new(http, url, "t", "d"));
}

/// JSON shaped like the answers: their keys, with values of any kind.
fn json() -> impl Strategy<Value = Value> {
    let leaf = prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::from),
        any::<i64>().prop_map(Value::from),
        any::<u64>().prop_map(Value::from),
        "[a-z/:.-]{0,12}".prop_map(Value::from),
    ];
    let keys = prop::sample::select(vec![
        "Items",
        "Id",
        "Type",
        "Name",
        "UserData",
        "RunTimeTicks",
        "MediaSources",
        "MediaStreams",
        "Codec",
        "Index",
        "MediaContainer",
        "Metadata",
        "Directory",
        "ratingKey",
        "type",
        "Media",
        "Part",
        "key",
        "Stream",
        "streamType",
        "totalSize",
        "TotalRecordCount",
        "thumb",
        "duration",
    ]);
    leaf.prop_recursive(5, 48, 6, move |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..4).prop_map(Value::from),
            prop::collection::btree_map(keys.clone(), inner, 0..6).prop_map(|m| {
                Value::Object(m.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
            }),
        ]
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    // T-SRV-013
    #[test]
    fn any_answer_is_an_error_or_a_page(value in json()) {
        both(value.to_string().into_bytes());
    }

    // T-SRV-013
    #[test]
    fn any_bytes_are_an_error(bytes in prop::collection::vec(any::<u8>(), 0..256)) {
        both(bytes);
    }

    #[test]
    fn any_time_parses_or_not(text in ".{0,40}") {
        let _ = parse_time(&text);
    }
}

#[test]
fn next_skips_what_cannot_be_played() {
    let item = |id: &str, kind| Item {
        id: id.into(),
        kind,
        title: String::new(),
        series: None,
        parent: None,
        season: None,
        episode: None,
        cover: None,
        played: false,
        position_ms: 0,
        duration_ms: 0,
        last_played_s: None,
        anilist_id: None,
    };
    let items = [
        item("a", ItemKind::Episode),
        item("b", ItemKind::Season),
        item("c", ItemKind::Episode),
    ];
    assert_eq!(next_of(&items, "a").map(|i| i.id.as_str()), Some("c"));
    assert!(next_of(&items, "c").is_none());
    assert!(next_of(&items, "z").is_none());
}
