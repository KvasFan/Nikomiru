// SPDX-License-Identifier: GPL-3.0-only

//! L4: the setup assistant's downloads against a fake host (T-INS-002…
//! T-INS-004, T-INS-006): Fushi's pack read by range, a library downloaded
//! across parts, resumed, checked; a dictionary checked by its SHA-256.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::Duration;

use nikomiru_core::Cancel;
use nikomiru_online::OnlineError;
use nikomiru_online::fake_files::{FakeFiles, zip64};
use nikomiru_online::http::Http;
use nikomiru_online::setup::fetch::{Pack, download_file, manifest, part_path};
use reqwest::Url;

/// Bytes that do not repeat, so a piece in the wrong place shows.
fn audio(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i * 7 + i / 251) as u8).collect()
}

fn pack(host: &FakeFiles, library: &[u8]) -> (Http, String) {
    let zip = zip64(&[
        ("backup_meta.json", b"{}"),
        ("fushi.db", b"labels"),
        ("dictionaryResources/Big/blobs.bin", &vec![9u8; 300_000]),
        ("localAudio/local_audio_1.db", library),
    ]);
    let url = host.put_pack(&zip, 100_000);
    (Http::new("").unwrap(), url)
}

// T-INS-003, T-INS-004: only the library is fetched, by range, across parts
#[test]
fn a_library_is_fetched_alone_across_the_parts() {
    let host = FakeFiles::start().unwrap();
    let library = audio(250_000);
    let (http, url) = pack(&host, &library);
    let m = manifest(&http, &[Url::parse(&url).unwrap()]).unwrap();
    assert_eq!(m.parts.len(), 6);
    let mut pack = Pack::new(&http, m);
    pack.backoff = Duration::ZERO;
    let cancel = Cancel::new();
    let entries = pack.directory(&cancel).unwrap();
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names[3], "localAudio/local_audio_1.db");
    assert_eq!(
        pack.small(&entries[1], 1 << 20, &cancel).unwrap(),
        b"labels"
    );
    host.clear_requests();
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("android.db");
    let mut seen = Vec::new();
    let got = pack
        .download(&entries[3], &dest, &cancel, &mut |done| seen.push(done))
        .unwrap();
    assert_eq!(std::fs::read(&got).unwrap(), library);
    assert!(!part_path(&dest).exists());
    assert_eq!(seen.last(), Some(&250_000));
    // only ranges of the parts that hold it, never the dictionaries' bytes
    let requests = host.requests();
    assert!(
        requests
            .iter()
            .all(|(path, range)| path.starts_with("/parts/") && range.is_some()),
        "{requests:?}"
    );
    let parts: std::collections::BTreeSet<&str> =
        requests.iter().map(|(p, _)| p.as_str()).collect();
    assert!(
        parts.len() >= 3 && !parts.contains("/parts/pack.zip.000"),
        "{parts:?}"
    );
}

// T-INS-004, T-INS-006: a part not there is asked of the whole pack; a
// download goes on from its `.part`; a wrong CRC-32 is refused
#[test]
fn a_download_resumes_and_is_checked() {
    let host = FakeFiles::start().unwrap();
    let library = audio(250_000);
    let (http, url) = pack(&host, &library);
    let mut pack = Pack::new(
        &http,
        manifest(&http, &[Url::parse(&url).unwrap()]).unwrap(),
    );
    pack.backoff = Duration::ZERO;
    let cancel = Cancel::new();
    let entry = pack.directory(&cancel).unwrap().pop().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("android.db");
    // stopped after 120 000 bytes
    std::fs::write(part_path(&dest), &library[..120_000]).unwrap();
    host.clear_requests();
    host.then_status("/parts/pack.zip.004", 503);
    pack.download(&entry, &dest, &cancel, &mut |_| {}).unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), library);
    let requests = host.requests();
    assert!(
        requests.iter().any(|(p, _)| p == "/whole.zip"),
        "{requests:?}"
    );
    // only what was missing: no range of the whole pack before it
    let start = pack.data_offset(&entry, &cancel).unwrap();
    let whole: Vec<u64> = requests
        .iter()
        .filter(|(p, _)| p == "/whole.zip")
        .filter_map(|(_, r)| {
            r.as_ref()?
                .strip_prefix("bytes=")?
                .split_once('-')?
                .0
                .parse()
                .ok()
        })
        .collect();
    assert!(
        whole.iter().all(|&from| from >= start + 120_000),
        "{whole:?}"
    );
    // a byte changed on the host: refused, nothing kept
    std::fs::remove_file(&dest).unwrap();
    host.with_file("/parts/pack.zip.005", |b| b[100] ^= 0xff);
    let failed = pack
        .download(&entry, &dest, &cancel, &mut |_| {})
        .unwrap_err();
    assert!(failed.to_string().contains("CRC-32"), "{failed}");
    assert!(!dest.exists() && !part_path(&dest).exists());
    // a compressed entry is refused before anything is asked
    let mut deflated = entry.clone();
    deflated.method = 8;
    host.clear_requests();
    let refused = pack
        .download(&deflated, &dest, &cancel, &mut |_| {})
        .unwrap_err();
    assert!(refused.to_string().contains("compressed"), "{refused}");
    assert!(host.requests().is_empty());
    // stopped: the `.part` stays
    cancel.cancel();
    assert!(matches!(
        pack.download(&entry, &dest, &cancel, &mut |_| {}),
        Err(OnlineError::Cancelled)
    ));
}

// T-INS-002: a dictionary checked against its SHA-256
#[test]
fn a_dictionary_is_checked_by_its_hash() {
    let host = FakeFiles::start().unwrap();
    host.put("/dict.zip", b"a dictionary".to_vec());
    let http = Http::new("").unwrap();
    let url = Url::parse(&host.url("/dict.zip")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("dict.zip");
    let other = "cf5ddfdc6f99c5a9a6c3e3ad7f0bd3e18a1fd7c8a40b6c64b7c7c0c4ef7c6f3e";
    let fetch = |sha256: Option<&str>| {
        download_file(
            &http,
            &url,
            sha256,
            &dest,
            Duration::ZERO,
            &Cancel::new(),
            &mut |_| {},
        )
    };
    let wrong = fetch(Some(other));
    assert!(wrong.is_err() && !dest.exists() && !part_path(&dest).exists());
    use sha2::Digest;
    let its: String = sha2::Sha256::digest(b"a dictionary")
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let got = fetch(Some(&its)).unwrap();
    assert_eq!(std::fs::read(got).unwrap(), b"a dictionary");
}

// AK-BUG-005: a gateway's 504, as GitHub answered for KANJIDIC and Kanjium,
// is tried again; a 404 is not, and three failures are one
#[test]
fn ak_bug_005_a_passing_failure_is_tried_again() {
    let host = FakeFiles::start().unwrap();
    host.put("/dict.zip", b"a dictionary".to_vec());
    let http = Http::new("").unwrap();
    let url = Url::parse(&host.url("/dict.zip")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("dict.zip");
    let fetch = || {
        download_file(
            &http,
            &url,
            None,
            &dest,
            Duration::ZERO,
            &Cancel::new(),
            &mut |_| {},
        )
    };
    host.then_status("/dict.zip", 504);
    host.then_status("/dict.zip", 502);
    assert_eq!(std::fs::read(fetch().unwrap()).unwrap(), b"a dictionary");
    std::fs::remove_file(&dest).unwrap();
    for _ in 0..3 {
        host.then_status("/dict.zip", 504);
    }
    host.clear_requests();
    assert!(matches!(
        fetch(),
        Err(OnlineError::Http { status: 504, .. })
    ));
    assert_eq!(host.requests().len(), 3);
    assert!(!dest.exists() && !part_path(&dest).exists());
    host.then_status("/dict.zip", 404);
    host.clear_requests();
    assert!(fetch().is_err());
    assert_eq!(host.requests().len(), 1);
}

// T-INS-003 against Fushi's real pack, by hand: the manifest, the central
// directory and `fushi.db`, about 1 MB in all
#[test]
#[ignore = "L7: reads Fushi's pack on the network"]
fn l7_the_real_pack_is_read_by_range() {
    use nikomiru_online::setup::fetch::MANIFEST_URLS;
    let http = Http::new("").unwrap();
    let urls: Vec<Url> = MANIFEST_URLS
        .iter()
        .map(|u| Url::parse(u).unwrap())
        .collect();
    let pack = Pack::new(&http, manifest(&http, &urls).unwrap());
    assert!(!pack.manifest.parts.is_empty());
    let cancel = Cancel::new();
    let entries = pack.directory(&cancel).unwrap();
    let audio: Vec<_> = entries
        .iter()
        .filter(|e| e.name.starts_with("localAudio/"))
        .collect();
    assert_eq!(audio.len(), 2, "{entries:?}");
    assert!(audio.iter().all(|e| e.method == 0));
    let db = entries.iter().find(|e| e.name == "fushi.db").unwrap();
    let bytes = pack.small(db, 8 << 20, &cancel).unwrap();
    assert!(bytes.starts_with(b"SQLite format 3\0"));
}
