// SPDX-License-Identifier: GPL-3.0-only

//! Podcast feeds (POD-001…POD-003): bounded inert RSS 2.0 reading, stable
//! identities, resumable offline downloads and feed transcripts. Nothing
//! here runs on its own: every request is an explicit action of the user.

use std::path::Path;

use quick_xml::NsReader;
use quick_xml::events::{BytesStart, Event};
use quick_xml::name::ResolveResult;
use reqwest::Url;
use sha2::{Digest, Sha256};

use nikomiru_core::Cancel;

use crate::OnlineError;
use crate::http::Http;

/// The largest feed.
pub const MAX_FEED_BYTES: u64 = 4 << 20;
/// Episodes kept from a feed.
pub const MAX_EPISODES: usize = 1000;
/// The largest episode.
pub const MAX_AUDIO: u64 = 2 << 30;
/// The largest transcript.
pub const MAX_TRANSCRIPT: u64 = 4 << 20;
/// The longest kept title, link or date.
pub const MAX_TEXT: usize = 1024;
/// Element nesting at most.
const MAX_DEPTH: usize = 64;
/// The Podcasting 2.0 namespace.
const PODCAST_NS: &[u8] = b"https://podcastindex.org/namespace/1.0";

/// An enclosed audio file.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Enclosure {
    /// Its address.
    pub url: String,
    /// Its declared MIME type.
    pub mime: String,
    /// Its declared length in bytes.
    pub length: Option<u64>,
}

/// A transcript the feed offers.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Transcript {
    /// Its address.
    pub url: String,
    /// Its declared MIME type.
    pub mime: String,
    /// Its language, if declared.
    pub language: String,
}

/// One item of a feed.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Item {
    /// Its GUID, if any.
    pub guid: String,
    /// Its title.
    pub title: String,
    /// Its web page.
    pub link: String,
    /// Its publication date, as written.
    pub published: String,
    /// Its audio.
    pub enclosure: Option<Enclosure>,
    /// Its transcripts.
    pub transcripts: Vec<Transcript>,
}

/// A feed read.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Feed {
    /// The channel's title.
    pub title: String,
    /// The channel's web page.
    pub link: String,
    /// Its items, in feed order, duplicates dropped.
    pub items: Vec<Item>,
}

fn hex16(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .take(16)
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// A feed's identity, from its address.
pub fn feed_id(url: &str) -> String {
    hex16(url.trim())
}

/// An episode's identity: its feed's address and GUID, or its audio's
/// address when it has no GUID.
pub fn episode_id(feed_url: &str, item: &Item) -> Option<String> {
    let key = if item.guid.is_empty() {
        item.enclosure.as_ref().map(|e| e.url.as_str())?
    } else {
        item.guid.as_str()
    };
    Some(hex16(&format!("{}\n{key}", feed_url.trim())))
}

fn bad(reason: &str) -> OnlineError {
    OnlineError::Unusable(format!("feed: {reason}"))
}

fn clean(text: &str) -> String {
    let text: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut end = text.len().min(MAX_TEXT);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_string()
}

fn reference(bytes: &[u8]) -> Result<char, OnlineError> {
    let s = std::str::from_utf8(bytes).map_err(|_| bad("reference"))?;
    match s {
        "amp" => Ok('&'),
        "lt" => Ok('<'),
        "gt" => Ok('>'),
        "apos" => Ok('\''),
        "quot" => Ok('"'),
        _ => s
            .strip_prefix("#x")
            .map(|h| u32::from_str_radix(h, 16).ok())
            .unwrap_or_else(|| s.strip_prefix('#').and_then(|d| d.parse().ok()))
            .and_then(char::from_u32)
            .ok_or_else(|| bad("unknown entity")),
    }
}

fn attribute(e: &BytesStart<'_>, name: &[u8]) -> Result<Option<String>, OnlineError> {
    for a in e.attributes() {
        let a = a.map_err(|_| bad("attribute"))?;
        if a.key.as_ref() == name {
            let value = a
                .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                .map_err(|_| bad("attribute"))?;
            return Ok(Some(value.trim().to_string()));
        }
    }
    Ok(None)
}

/// What a text is collected for.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Field {
    ChannelTitle,
    ChannelLink,
    Title,
    Link,
    Guid,
    Published,
}

/// Reads an RSS 2.0 feed of at most [`MAX_FEED_BYTES`]: titles, links,
/// dates, enclosures and Podcasting 2.0 transcripts. DTDs and entities
/// other than XML's are refused; nothing in it is fetched or run.
pub fn parse(bytes: &[u8]) -> Result<Feed, OnlineError> {
    if bytes.len() as u64 > MAX_FEED_BYTES {
        return Err(OnlineError::TooLarge {
            limit_mib: MAX_FEED_BYTES >> 20,
        });
    }
    let text = std::str::from_utf8(bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes))
        .map_err(|_| bad("not UTF-8"))?;
    let mut r = NsReader::from_str(text);
    let mut feed = Feed::default();
    let mut stack: Vec<Vec<u8>> = Vec::new();
    let mut item: Option<Item> = None;
    let mut field: Option<(Field, usize, String)> = None;
    let mut seen_rss = false;
    let mut keys = std::collections::HashSet::new();
    loop {
        let event = r.read_event().map_err(|_| bad("malformed XML"))?;
        match event {
            Event::Start(ref e) | Event::Empty(ref e) => {
                let empty = matches!(event, Event::Empty(_));
                let (ns, local) = r.resolver().resolve_element(e.name());
                let plain = matches!(ns, ResolveResult::Unbound);
                let podcast = matches!(&ns, ResolveResult::Bound(n) if n.as_ref() == PODCAST_NS);
                let local = local.as_ref().to_vec();
                if stack.is_empty() {
                    if !(plain && local == b"rss") {
                        return Err(bad("not RSS"));
                    }
                    seen_rss = true;
                }
                let parent = stack.last().map(Vec::as_slice);
                let in_item = item.is_some();
                if plain && local == b"item" && parent == Some(b"channel".as_slice()) && !empty {
                    item = Some(Item::default());
                } else if plain && in_item && local == b"enclosure" {
                    if let Some(i) = &mut item
                        && i.enclosure.is_none()
                        && let Some(url) = attribute(e, b"url")?
                    {
                        i.enclosure = Some(Enclosure {
                            url: clean(&url),
                            mime: clean(&attribute(e, b"type")?.unwrap_or_default()),
                            length: attribute(e, b"length")?.and_then(|l| l.parse().ok()),
                        });
                    }
                } else if podcast && in_item && local == b"transcript" {
                    if let Some(i) = &mut item
                        && i.transcripts.len() < 8
                        && let Some(url) = attribute(e, b"url")?
                    {
                        i.transcripts.push(Transcript {
                            url: clean(&url),
                            mime: clean(&attribute(e, b"type")?.unwrap_or_default()),
                            language: clean(&attribute(e, b"language")?.unwrap_or_default()),
                        });
                    }
                } else if plain && !empty && field.is_none() {
                    let which = match (in_item, parent, local.as_slice()) {
                        (false, Some(b"channel"), b"title") => Some(Field::ChannelTitle),
                        (false, Some(b"channel"), b"link") => Some(Field::ChannelLink),
                        (true, Some(b"item"), b"title") => Some(Field::Title),
                        (true, Some(b"item"), b"link") => Some(Field::Link),
                        (true, Some(b"item"), b"guid") => Some(Field::Guid),
                        (true, Some(b"item"), b"pubDate") => Some(Field::Published),
                        _ => None,
                    };
                    if let Some(which) = which {
                        field = Some((which, stack.len() + 1, String::new()));
                    }
                }
                if !empty {
                    stack.push(local);
                    if stack.len() > MAX_DEPTH {
                        return Err(bad("too deep"));
                    }
                }
            }
            Event::End(_) => {
                let depth = stack.len();
                if let Some((which, at, text)) = field.take_if(|(_, at, _)| *at == depth) {
                    let _ = at;
                    let text = clean(&text);
                    match (which, &mut item) {
                        (Field::ChannelTitle, _) => feed.title = text,
                        (Field::ChannelLink, _) => feed.link = text,
                        (Field::Title, Some(i)) => i.title = text,
                        (Field::Link, Some(i)) => i.link = text,
                        (Field::Guid, Some(i)) => i.guid = text,
                        (Field::Published, Some(i)) => i.published = text,
                        _ => {}
                    }
                }
                let closed = stack.pop();
                if closed.as_deref() == Some(b"item".as_slice())
                    && stack.last().map(Vec::as_slice) == Some(b"channel".as_slice())
                    && let Some(done) = item.take()
                {
                    let key = if done.guid.is_empty() {
                        done.enclosure.as_ref().map(|e| e.url.clone())
                    } else {
                        Some(done.guid.clone())
                    };
                    if let Some(key) = key
                        && feed.items.len() < MAX_EPISODES
                        && keys.insert(key)
                    {
                        feed.items.push(done);
                    }
                }
            }
            Event::Text(e) => {
                if let Some((_, _, text)) = &mut field {
                    let piece = e
                        .xml_content(quick_xml::XmlVersion::Implicit1_0)
                        .map_err(|_| bad("text"))?;
                    if text.len() < 4 * MAX_TEXT {
                        text.push_str(&piece);
                    }
                }
            }
            Event::CData(e) => {
                if let Some((_, _, text)) = &mut field {
                    let piece = e
                        .xml_content(quick_xml::XmlVersion::Implicit1_0)
                        .map_err(|_| bad("text"))?;
                    if text.len() < 4 * MAX_TEXT {
                        text.push_str(&piece);
                    }
                }
            }
            Event::GeneralRef(e) => {
                let c = reference(e.as_ref())?;
                if let Some((_, _, text)) = &mut field {
                    text.push(c);
                }
            }
            Event::DocType(_) => return Err(bad("DTD")),
            Event::Eof => break,
            _ => {}
        }
    }
    if !seen_rss || !stack.is_empty() {
        return Err(bad("incomplete"));
    }
    if feed.title.is_empty() {
        return Err(bad("no channel title"));
    }
    Ok(feed)
}

/// The file extension of an episode's audio, from its declared type or its
/// address; `None` if it is not audio Nikomiru plays.
pub fn audio_extension(enclosure: &Enclosure) -> Option<&'static str> {
    let mime = enclosure.mime.to_ascii_lowercase();
    let by_type = match mime.split(';').next().unwrap_or_default().trim() {
        "audio/mpeg" | "audio/mp3" => Some("mp3"),
        "audio/mp4" | "audio/x-m4a" | "audio/m4a" => Some("m4a"),
        "audio/aac" | "audio/x-aac" => Some("aac"),
        "audio/ogg" | "audio/vorbis" => Some("ogg"),
        "audio/opus" => Some("opus"),
        "audio/flac" | "audio/x-flac" => Some("flac"),
        "audio/wav" | "audio/x-wav" | "audio/wave" => Some("wav"),
        _ => None,
    };
    by_type.or_else(|| {
        let url = Url::parse(&enclosure.url).ok()?;
        let name = url.path_segments()?.next_back()?.to_ascii_lowercase();
        let ext = name.rsplit_once('.')?.1.to_string();
        ["mp3", "m4a", "aac", "ogg", "opus", "flac", "wav"]
            .into_iter()
            .find(|e| *e == ext)
    })
}

/// The sidecar extension of a transcript Nikomiru reads, or `None`.
pub fn transcript_extension(mime: &str) -> Option<&'static str> {
    match mime.to_ascii_lowercase().split(';').next()?.trim() {
        "text/vtt" => Some("vtt"),
        "application/x-subrip" | "application/srt" | "text/srt" | "text/x-srt" => Some("srt"),
        _ => None,
    }
}

/// Reads a small file whole, at most `limit` bytes.
fn small(http: &Http, url: &Url, limit: u64, cancel: &Cancel) -> Result<Vec<u8>, OnlineError> {
    let mut out = Vec::new();
    http.stream_from(url, "podcast", 0, limit, &mut |_, _| Ok(()), &mut |piece| {
        if cancel.is_cancelled() {
            return Err(OnlineError::Cancelled);
        }
        out.extend_from_slice(piece);
        Ok(())
    })?;
    Ok(out)
}

/// Fetches and reads a feed (POD-001).
pub fn fetch(http: &Http, url: &str, cancel: &Cancel) -> Result<Feed, OnlineError> {
    let url = crate::http::checked_url(url)?;
    parse(&small(http, &url, MAX_FEED_BYTES, cancel)?)
}

/// Fetches a transcript (POD-003).
pub fn transcript(http: &Http, url: &str, cancel: &Cancel) -> Result<Vec<u8>, OnlineError> {
    let url = crate::http::checked_url(url)?;
    small(http, &url, MAX_TRANSCRIPT, cancel)
}

/// Downloads `url` to `dest` through `dest.part` (POD-002): what an earlier
/// attempt left there is resumed by a range request, or started again when
/// the server answers the whole file. The length is checked before the
/// rename. Cancelled or failed, the partial file stays for the next try.
pub fn download(
    http: &Http,
    url: &str,
    dest: &Path,
    cancel: &Cancel,
    progress: &mut dyn FnMut(u64, Option<u64>),
) -> Result<u64, OnlineError> {
    let url = crate::http::checked_url(url)?;
    crate::setup::fetch::resumable(http, &url, dest, MAX_AUDIO, cancel, progress)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake_files::FakeFiles;

    const FEED: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0" xmlns:podcast="https://podcastindex.org/namespace/1.0" xmlns:itunes="http://www.itunes.com/dtds/podcast-1.0.dtd">
<channel>
  <title>日本語 &amp; Talk</title>
  <link>https://example.org/</link>
  <itunes:title>Not this</itunes:title>
  <item>
    <title><![CDATA[第1回 <b>猫</b>]]></title>
    <link>https://example.org/1</link>
    <guid isPermaLink="false">ep-1</guid>
    <pubDate>Mon, 16 Nov 2026 10:00:00 +0000</pubDate>
    <enclosure url="https://example.org/1.mp3" type="audio/mpeg" length="1234"/>
    <podcast:transcript url="https://example.org/1.vtt" type="text/vtt" language="ja"/>
    <podcast:transcript url="https://example.org/1.json" type="application/json"/>
  </item>
  <item>
    <title>Again</title>
    <guid>ep-1</guid>
    <enclosure url="https://example.org/dup.mp3" type="audio/mpeg"/>
  </item>
  <item>
    <title>No guid&#x2603;</title>
    <enclosure url="https://example.org/2.m4a?x=1" type=""/>
  </item>
</channel>
</rss>"#;

    // T-POD-001: real RSS with namespaces, entities, CDATA, duplicates and identities.
    #[test]
    fn a_feed_is_read_inertly() {
        let feed = parse(FEED.as_bytes()).unwrap();
        assert_eq!(feed.title, "日本語 & Talk");
        assert_eq!(feed.link, "https://example.org/");
        assert_eq!(feed.items.len(), 2, "the duplicate GUID is dropped");
        let first = &feed.items[0];
        assert_eq!(first.title, "第1回 <b>猫</b>", "plain text, never markup");
        assert_eq!(first.guid, "ep-1");
        assert_eq!(first.enclosure.as_ref().unwrap().length, Some(1234));
        assert_eq!(first.transcripts.len(), 2);
        assert_eq!(first.transcripts[0].language, "ja");
        assert_eq!(feed.items[1].title, "No guid☃");
        let url = "https://example.org/feed.xml";
        let a = episode_id(url, first).unwrap();
        assert_eq!(a, episode_id(url, first).unwrap());
        assert_ne!(a, episode_id("https://other/feed", first).unwrap());
        let b = episode_id(url, &feed.items[1]).unwrap();
        assert_ne!(a, b, "no GUID: the audio's address");
        assert_eq!(feed_id(url).len(), 32);
        assert_eq!(
            audio_extension(first.enclosure.as_ref().unwrap()),
            Some("mp3")
        );
        assert_eq!(
            audio_extension(feed.items[1].enclosure.as_ref().unwrap()),
            Some("m4a")
        );
        assert_eq!(transcript_extension("text/vtt"), Some("vtt"));
        assert_eq!(transcript_extension("application/json"), None);
    }

    // T-POD-001: DTDs, unknown entities, malformed or foreign XML and size.
    #[test]
    fn bad_feeds_are_refused() {
        for bad in [
            "<!DOCTYPE rss [<!ENTITY x \"y\">]><rss><channel><title>t</title></channel></rss>",
            "<rss><channel><title>&x;</title></channel></rss>",
            "<rss><channel><title>t</title></channel>",
            "<feed><title>atom</title></feed>",
            "<rss><channel></channel></rss>",
            "not xml",
        ] {
            assert!(parse(bad.as_bytes()).is_err(), "{bad}");
        }
        let big = vec![b' '; MAX_FEED_BYTES as usize + 1];
        assert!(matches!(parse(&big), Err(OnlineError::TooLarge { .. })));
        let mut many = String::from("<rss><channel><title>t</title>");
        for i in 0..MAX_EPISODES + 5 {
            many.push_str(&format!("<item><guid>{i}</guid></item>"));
        }
        many.push_str("</channel></rss>");
        assert_eq!(parse(many.as_bytes()).unwrap().items.len(), MAX_EPISODES);
    }

    // T-POD-002: a cut transfer is resumed from its partial file by a range.
    #[test]
    fn downloads_resume_where_they_stopped() {
        let host = FakeFiles::start().unwrap();
        let audio: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
        host.put("/ep.mp3", audio.clone());
        host.cut_after("/ep.mp3", 70_000);
        let http = Http::new("").unwrap();
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("p/ep.mp3");
        let mut seen = Vec::new();
        let cut = download(
            &http,
            &host.url("/ep.mp3"),
            &dest,
            &Cancel::new(),
            &mut |d, t| {
                seen.push((d, t));
            },
        );
        assert!(cut.is_err(), "{cut:?}");
        assert!(!dest.exists());
        let part = crate::setup::fetch::part_path(&dest);
        let kept = std::fs::metadata(&part).unwrap().len();
        assert!(kept > 0 && kept < audio.len() as u64, "{kept}");
        host.clear_requests();
        let size = download(
            &http,
            &host.url("/ep.mp3"),
            &dest,
            &Cancel::new(),
            &mut |_, _| {},
        )
        .unwrap();
        assert_eq!(size, audio.len() as u64);
        assert_eq!(std::fs::read(&dest).unwrap(), audio);
        assert!(!part.exists());
        assert_eq!(
            host.requests(),
            vec![("/ep.mp3".to_string(), Some(format!("bytes={kept}-")))]
        );
    }

    // T-POD-002: a server without ranges restarts from zero; cancel keeps the part.
    #[test]
    fn downloads_restart_or_stay_partial() {
        let host = FakeFiles::start().unwrap();
        let audio = vec![7u8; 100_000];
        host.put("/a.mp3", audio.clone());
        host.ignore_ranges("/a.mp3");
        let http = Http::new("").unwrap();
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("a.mp3");
        let part = crate::setup::fetch::part_path(&dest);
        std::fs::write(&part, vec![1u8; 500]).unwrap();
        download(
            &http,
            &host.url("/a.mp3"),
            &dest,
            &Cancel::new(),
            &mut |_, _| {},
        )
        .unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), audio, "started again");
        let cancel = Cancel::new();
        cancel.cancel();
        let other = dir.path().join("b.mp3");
        host.put("/b.mp3", audio);
        assert!(matches!(
            download(&http, &host.url("/b.mp3"), &other, &cancel, &mut |_, _| {}),
            Err(OnlineError::Cancelled)
        ));
        assert!(crate::setup::fetch::part_path(&other).exists());
        assert!(!other.exists());
        host.then_status("/c.mp3", 404);
        assert!(
            download(
                &http,
                &host.url("/c.mp3"),
                &dir.path().join("c.mp3"),
                &Cancel::new(),
                &mut |_, _| {}
            )
            .is_err()
        );
        assert!(
            download(
                &http,
                "http://example.org/x.mp3",
                &dest,
                &Cancel::new(),
                &mut |_, _| {}
            )
            .is_err(),
            "no clear HTTP"
        );
    }

    // T-POD-001, T-POD-003: feeds and transcripts come over the network bounded.
    #[test]
    fn feeds_and_transcripts_are_fetched() {
        let host = FakeFiles::start().unwrap();
        host.put("/feed.xml", FEED.as_bytes().to_vec());
        host.put(
            "/1.vtt",
            b"WEBVTT\n\n00:00.000 --> 00:01.000\n\xe7\x8c\xab\n".to_vec(),
        );
        let http = Http::new("").unwrap();
        let feed = fetch(&http, &host.url("/feed.xml"), &Cancel::new()).unwrap();
        assert_eq!(feed.items.len(), 2);
        let text = transcript(&http, &host.url("/1.vtt"), &Cancel::new()).unwrap();
        assert!(text.starts_with(b"WEBVTT"));
        host.put("/big.vtt", vec![b'a'; MAX_TRANSCRIPT as usize + 1]);
        assert!(transcript(&http, &host.url("/big.vtt"), &Cancel::new()).is_err());
    }
}
