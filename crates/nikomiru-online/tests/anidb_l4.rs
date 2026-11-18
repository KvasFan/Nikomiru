// SPDX-License-Identifier: GPL-3.0-only

//! L4 tests of AniDB file identification (MET-006) against a fake UDP API.

#![cfg(feature = "test-support")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use nikomiru_online::anidb::{self, Account, AniDb, CHUNK};
use nikomiru_online::error::OnlineError;
use nikomiru_online::fake_anidb::{CLIENT, FakeAniDb, PASSWORD, SESSION, USER};

fn account(password: &str) -> Account {
    Account {
        user: USER.into(),
        password: password.into(),
        client: CLIENT.into(),
        version: 1,
    }
}

const LINE: &str = "7|11|22|12|2019|Kimetsu no Yaiba|鬼滅の刃|Demon Slayer|1|Cruelty";

/// A client of `fake` whose waits are recorded, not slept.
fn client(fake: &FakeAniDb, password: &str) -> (AniDb, Arc<Mutex<Vec<Duration>>>) {
    let waits = Arc::new(Mutex::new(Vec::new()));
    let kept = waits.clone();
    let anidb = AniDb::new(
        fake.address(),
        account(password),
        Duration::from_secs(4),
        Box::new(move |d| kept.lock().unwrap().push(d)),
    )
    .unwrap();
    (anidb, waits)
}

fn file_of(bytes: &[u8]) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("[Group] Show - 01.mkv");
    std::fs::write(&path, bytes).unwrap();
    (dir, path)
}

// T-MET-006: a file AniDB knows: logged in, asked by size and hash with the
// masks, packets spaced, logged out at the end.
#[test]
fn a_known_file_is_identified() {
    let fake = FakeAniDb::start().unwrap();
    let (_dir, path) = file_of(b"episode one");
    let (hash, _) = anidb::ed2k(&mut &b"episode one"[..], 11, &|| false).unwrap();
    fake.data().files.insert((11, hash.clone()), LINE.into());
    let (mut anidb, waits) = client(&fake, PASSWORD);
    let info = anidb::identify(&mut anidb, &path, &|| false)
        .unwrap()
        .unwrap();
    assert_eq!((info.aid, info.epno.as_str()), (11, "1"));
    assert_eq!(info.titles()[0], "Kimetsu no Yaiba");
    drop(anidb);
    let packets = fake.data().packets.clone();
    assert_eq!(packets.len(), 3, "{packets:?}");
    assert!(packets[0].starts_with("AUTH user=learner&pass=pa&amp;ss&protover=3"));
    assert!(packets[0].contains(&format!("client={CLIENT}&clientver=1&enc=UTF8")));
    assert_eq!(
        packets[1],
        format!(
            "FILE size=11&ed2k={hash}&fmask={}&amask={}&s={SESSION}",
            anidb::FMASK,
            anidb::AMASK
        )
    );
    assert_eq!(packets[2], format!("LOGOUT s={SESSION}"));
    // the packets after the first wait their turn
    let waits = waits.lock().unwrap().clone();
    assert_eq!(waits.len(), 2);
    assert!(waits.iter().all(|w| *w > Duration::from_secs(3)));
}

// T-MET-006: an unknown file is no answer; one of whole chunks is asked
// again with the other ed2k variant.
#[test]
fn unknown_files_and_the_other_variant() {
    let fake = FakeAniDb::start().unwrap();
    let (_dir, path) = file_of(b"nobody knows");
    let (mut anidb, _) = client(&fake, PASSWORD);
    assert_eq!(anidb::identify(&mut anidb, &path, &|| false).unwrap(), None);
    let zeros = vec![0u8; CHUNK as usize];
    let (_dir2, whole) = file_of(&zeros);
    fake.data().files.insert(
        (CHUNK, "fc21d9af828f92a8df64beac3357425d".into()),
        LINE.into(),
    );
    let info = anidb::identify(&mut anidb, &whole, &|| false).unwrap();
    assert_eq!(info.map(|i| i.aid), Some(11));
    let files: Vec<String> = fake
        .data()
        .packets
        .iter()
        .filter(|p| p.starts_with("FILE"))
        .cloned()
        .collect();
    assert_eq!(files.len(), 3, "the usual hash, then the other: {files:?}");
    assert!(files[1].contains("d7def262a127cd79096a108e7a9fc138"));
    assert!(files[2].contains("fc21d9af828f92a8df64beac3357425d"));
}

// T-MET-006: a wrong password, a ban, an outdated client and an expired
// session.
#[test]
fn refusals_are_errors() {
    let fake = FakeAniDb::start().unwrap();
    let (_dir, path) = file_of(b"x");
    let (mut anidb, _) = client(&fake, "wrong");
    assert!(matches!(
        anidb::identify(&mut anidb, &path, &|| false),
        Err(OnlineError::Unauthorized { .. })
    ));
    fake.data().banned = true;
    let (mut anidb, _) = client(&fake, PASSWORD);
    assert!(matches!(
        anidb.file(1, "00"),
        Err(OnlineError::Protocol { .. })
    ));
    fake.data().banned = false;
    let mut outdated = AniDb::new(
        fake.address(),
        Account {
            version: 2,
            ..account(PASSWORD)
        },
        Duration::ZERO,
        Box::new(|_| {}),
    )
    .unwrap();
    assert!(matches!(
        outdated.file(1, "00"),
        Err(OnlineError::Protocol { .. })
    ));
    // a session that ended is opened again, once
    let (mut anidb, _) = client(&fake, PASSWORD);
    fake.data().expire_once = true;
    assert_eq!(anidb.file(1, "00").unwrap(), None);
    // no client or no account: nothing is sent
    let before = fake.data().packets.len();
    for (client, user) in [("", USER), (CLIENT, "")] {
        let a = Account {
            client: client.into(),
            user: user.into(),
            ..account(PASSWORD)
        };
        assert!(matches!(
            AniDb::new(fake.address(), a, Duration::ZERO, Box::new(|_| {})),
            Err(OnlineError::NoKey { .. })
        ));
    }
    drop(anidb);
    drop(outdated);
    // only the logout of the session that was open
    assert_eq!(fake.data().packets.len(), before + 1);
}
