// SPDX-License-Identifier: GPL-3.0-only

//! Spike S3 (D-2): an android.db of the Local Audio Server for Yomichan read
//! with `turso`, read-only, as AUD-003 needs: the exact pair, then the
//! expression alone, the sub-source by priority, then the audio blob.
//!
//!     spike-s3-local-audio DB WORDS
//!
//! Prints the time to open, of the first query, and p50/p95/p99/max of the
//! lookups; checks that a blob comes back whole.

use std::future::Future;
use std::task::{Context, Poll, Waker};
use std::time::{Duration, Instant};

/// Drives a future of turso to its end: they step their own I/O when polled.
fn block_on<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    let mut cx = Context::from_waker(Waker::noop());
    loop {
        if let Poll::Ready(v) = future.as_mut().poll(&mut cx) {
            return v;
        }
        std::thread::yield_now();
    }
}

/// The sub-sources in the order they are preferred (AUD-003).
const PRIORITY: [&str; 4] = ["nhk16", "shinmeikai8", "jpod", "forvo"];

async fn candidates(
    conn: &turso::Connection,
    expression: &str,
    reading: &str,
) -> turso::Result<Vec<(String, String)>> {
    let mut out = Vec::new();
    let mut rows = conn
        .query(
            "SELECT file, source FROM entries WHERE expression = ?1 AND reading = ?2",
            (expression, reading),
        )
        .await?;
    while let Some(row) = rows.next().await? {
        out.push((row.get::<String>(0)?, row.get::<String>(1)?));
    }
    if out.is_empty() {
        let mut rows = conn
            .query(
                "SELECT file, source FROM entries WHERE expression = ?1",
                (expression,),
            )
            .await?;
        while let Some(row) = rows.next().await? {
            out.push((row.get::<String>(0)?, row.get::<String>(1)?));
        }
    }
    Ok(out)
}

async fn blob(conn: &turso::Connection, file: &str, source: &str) -> turso::Result<Option<Vec<u8>>> {
    let mut rows = conn
        .query(
            "SELECT data FROM android WHERE file = ?1 AND source = ?2 LIMIT 1",
            (file, source),
        )
        .await?;
    match rows.next().await? {
        Some(row) => Ok(Some(row.get::<Vec<u8>>(0)?)),
        None => Ok(None),
    }
}

async fn lookup(
    conn: &turso::Connection,
    expression: &str,
    reading: &str,
) -> turso::Result<Option<(String, usize)>> {
    let found = candidates(conn, expression, reading).await?;
    let best = found
        .into_iter()
        .filter_map(|(f, s)| PRIORITY.iter().position(|p| *p == s).map(|r| (r, f, s)))
        .min_by_key(|(r, _, _)| *r);
    let Some((_, file, source)) = best else {
        return Ok(None);
    };
    Ok(blob(conn, &file, &source).await?.map(|b| (source, b.len())))
}

fn percentile(sorted: &[Duration], p: f64) -> Duration {
    let i = ((sorted.len() as f64 - 1.0) * p).round() as usize;
    sorted[i.min(sorted.len() - 1)]
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (db_path, words_path) = (&args[1], &args[2]);
    let words: Vec<(String, String)> = std::fs::read_to_string(words_path)
        .expect("words")
        .lines()
        .filter_map(|l| l.split_once('\t'))
        .map(|(e, r)| (e.to_string(), r.to_string()))
        .collect();
    let started = Instant::now();
    let db = block_on(turso::Builder::new_local(db_path).read_only(true).build()).expect("open");
    let conn = db.connect().expect("connect");
    let opened = started.elapsed();
    let mut times = Vec::new();
    let (mut found, mut missed, mut bytes) = (0usize, 0usize, 0usize);
    let mut first = None;
    for (e, r) in &words {
        let t = Instant::now();
        match block_on(lookup(&conn, e, r)).expect("lookup") {
            Some((_, n)) => {
                found += 1;
                bytes += n;
            }
            None => missed += 1,
        }
        let took = t.elapsed();
        first.get_or_insert(took);
        times.push(took);
    }
    times.sort();
    println!("open: {opened:?}");
    println!("first lookup: {:?}", first.unwrap_or_default());
    println!(
        "{} lookups ({found} found, {missed} not, {} KiB of audio)",
        times.len(),
        bytes >> 10
    );
    for (name, p) in [("p50", 0.5), ("p95", 0.95), ("p99", 0.99), ("max", 1.0)] {
        println!("{name}: {:?}", percentile(&times, p));
    }
    // a write is refused on a read-only open
    let write = block_on(conn.execute("CREATE TABLE x (a)", ()));
    println!("write refused: {}", write.is_err());
}
