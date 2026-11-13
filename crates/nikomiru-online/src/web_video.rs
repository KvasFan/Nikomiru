// SPDX-License-Identifier: GPL-3.0-only

//! Validate the media selected by yt-dlp without exposing expiring addresses
//! or their headers to the player (PLY-023). The app runs the executable.

use crate::{http::checked_url, servers::Stream};
use serde_json::Value;
use sha2::{Digest, Sha256};

/// Media metadata needed by the player.
#[derive(Debug)]
pub struct Video {
    /// Display title.
    pub title: String,
    /// The video, with its audio when multiplexed.
    pub video: Stream,
    /// A separate audio stream, if the selected format has one.
    pub audio: Option<Stream>,
}

/// Stable identifier of the original page, safe for source links (ANK-050).
pub fn uid(page: &str) -> String {
    format!(
        "web-{}",
        Sha256::digest(page.trim().as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    )
}

/// Read one yt-dlp JSON result, accepting only ordinary HTTP media. The
/// original page URL is supplied by the caller, never taken from this JSON.
pub fn parse(json: &str) -> Result<Video, String> {
    let value: Value = serde_json::from_str(json).map_err(|_| "invalid metadata")?;
    if value.get("entries").is_some() || value["is_live"] == true || value["has_drm"] == true {
        return Err("not an on-demand video".into());
    }
    let stream = |v: &Value| -> Result<Stream, String> {
        let protocol = v["protocol"].as_str().unwrap_or("https");
        if !matches!(protocol, "http" | "https") {
            return Err("segmented stream".into());
        }
        let url = checked_url(v["url"].as_str().ok_or("no media address")?)
            .map_err(|_| "invalid media address")?;
        if !url.username().is_empty() || url.password().is_some() {
            return Err("credentials in address".into());
        }
        let mut headers = std::collections::BTreeMap::new();
        for h in [&value["http_headers"], &v["http_headers"]] {
            if let Some(h) = h.as_object() {
                for (name, v) in h {
                    if let Some(v) = v.as_str() {
                        if name.contains(['\r', '\n']) || v.contains(['\r', '\n']) {
                            return Err("invalid header".into());
                        }
                        headers.insert(name.clone(), v.to_string());
                    }
                }
            }
        }
        Ok(Stream {
            url,
            headers: headers.into_iter().collect(),
        })
    };
    let (video, audio) = if let Some(formats) = value["requested_formats"].as_array() {
        let v = formats
            .iter()
            .find(|v| v["vcodec"].as_str().is_some_and(|c| c != "none"))
            .ok_or("no video")?;
        let audio = formats
            .iter()
            .find(|a| a["vcodec"] == "none" && a["acodec"].as_str().is_some_and(|c| c != "none"));
        (stream(v)?, audio.map(stream).transpose()?)
    } else {
        (stream(&value)?, None)
    };
    Ok(Video {
        title: value["title"]
            .as_str()
            .unwrap_or("Web video")
            .chars()
            .take(1000)
            .collect(),
        video,
        audio,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    // T-PLY-023
    #[test]
    fn separate_streams_keep_headers_and_reject_non_http_media() {
        let v=parse(r#"{"title":"日本語","http_headers":{"User-Agent":"test"},"requested_formats":[{"url":"https://media.example/video","protocol":"https","vcodec":"av1","acodec":"none"},{"url":"https://media.example/audio","protocol":"https","vcodec":"none","acodec":"opus"}]}"#).unwrap();
        assert_eq!(v.title, "日本語");
        assert!(v.audio.is_some());
        assert_eq!(v.video.headers[0], ("User-Agent".into(), "test".into()));
        for json in [
            r#"{"url":"file:///etc/passwd"}"#,
            r#"{"url":"https://example.org/list","protocol":"m3u8_native"}"#,
            r#"{"url":"https://example.org/a","is_live":true}"#,
            r#"{"entries":[]}"#,
        ] {
            assert!(parse(json).is_err());
        }
        assert_eq!(
            uid(" https://example.org/watch?v=1 "),
            uid("https://example.org/watch?v=1")
        );
        assert_ne!(
            uid("https://example.org/watch?v=1"),
            uid("https://example.org/watch?v=2")
        );
    }
}
