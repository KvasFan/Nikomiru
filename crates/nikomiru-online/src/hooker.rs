// SPDX-License-Identifier: GPL-3.0-only

//! Text from a hooker (HOK-001…HOK-003): Textractor's or LunaTranslator's
//! WebSocket on this computer, which sends each line of a game as it shows.
//! What comes is untrusted text, read with limits.

use std::net::TcpStream;
use std::time::Duration;

use reqwest::Url;
use serde_json::Value;
use tungstenite::protocol::{Message, WebSocket, WebSocketConfig};

use crate::error::OnlineError;
use crate::http::is_loopback;

/// The longest line kept, in characters.
pub const LINE_LIMIT: usize = 4096;
/// The largest message read.
const MESSAGE_LIMIT: usize = 1 << 20;
/// How long a read waits, so that a stop is heard.
const READ_WAIT: Duration = Duration::from_millis(250);

/// The address, if it is a WebSocket on this computer (HOK-001).
pub fn checked_hook_url(url: &str) -> Result<Url, OnlineError> {
    let bad = || OnlineError::BadUrl(nikomiru_core::config::without_userinfo(url));
    let parsed = Url::parse(url.trim()).map_err(|_| bad())?;
    if parsed.scheme() == "ws" && is_loopback(&parsed) {
        Ok(parsed)
    } else {
        Err(bad())
    }
}

/// The text of a line, cleaned, without what would break the page: control
/// characters go, runs of blanks shrink, and long lines are cut.
pub fn clean(text: &str) -> Option<String> {
    let mut out = String::new();
    let mut count = 0;
    for word in text.split_whitespace() {
        if !out.is_empty() {
            out.push(' ');
        }
        for c in word.chars().filter(|c| !c.is_control()) {
            if count == LINE_LIMIT {
                return Some(out);
            }
            out.push(c);
            count += 1;
        }
    }
    (!out.is_empty()).then_some(out)
}

/// The line in a message: plain text, or JSON with a `sentence` or `text`.
pub fn line_of(message: &str) -> Option<String> {
    if message.trim_start().starts_with('{')
        && let Ok(value) = serde_json::from_str::<Value>(message)
    {
        let text = value["sentence"].as_str().or(value["text"].as_str())?;
        return clean(text);
    }
    clean(message)
}

/// Whether a text is one a Japanese game would show: the clipboard holds all
/// sorts of things, passwords among them, and only such text is kept (HOK-003).
pub fn is_japanese(text: &str) -> bool {
    text.chars()
        .any(|c| matches!(c, '\u{3040}'..='\u{30ff}' | '\u{3400}'..='\u{4dbf}' | '\u{4e00}'..='\u{9fff}'))
}

/// A connection to a hooker.
pub struct Hook {
    socket: WebSocket<TcpStream>,
}

impl std::fmt::Debug for Hook {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Hook").finish_non_exhaustive()
    }
}

impl Hook {
    /// Connects.
    pub fn connect(url: &Url) -> Result<Self, OnlineError> {
        let host = url.host_str().unwrap_or_default().to_string();
        let port = url.port().unwrap_or(80);
        let unreachable = |reason: String| OnlineError::Unreachable {
            host: host.clone(),
            reason,
        };
        let stream =
            TcpStream::connect((host.as_str(), port)).map_err(|e| unreachable(e.to_string()))?;
        let config = WebSocketConfig::default().max_message_size(Some(MESSAGE_LIMIT));
        let (socket, _) =
            tungstenite::client::client_with_config(url.as_str(), stream, Some(config))
                .map_err(|e| unreachable(e.to_string()))?;
        socket
            .get_ref()
            .set_read_timeout(Some(READ_WAIT))
            .map_err(|e| unreachable(e.to_string()))?;
        Ok(Self { socket })
    }

    /// The next line, or `None` when none came in time. An error means the
    /// connection is over.
    pub fn read(&mut self) -> Result<Option<String>, OnlineError> {
        match self.socket.read() {
            Ok(Message::Text(text)) => Ok(line_of(text.as_str())),
            Ok(_) => Ok(None),
            Err(tungstenite::Error::Io(e))
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                Ok(None)
            }
            Err(e) => Err(OnlineError::Unreachable {
                host: "hooker".into(),
                reason: e.to_string(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-HOK-001
    #[test]
    fn only_a_websocket_on_this_computer() {
        assert!(checked_hook_url("ws://127.0.0.1:6677").is_ok());
        assert!(checked_hook_url("ws://localhost:6677/x").is_ok());
        for bad in [
            "ws://192.168.1.2:6677",
            "wss://127.0.0.1",
            "http://127.0.0.1",
            "ws://example.org",
            "",
        ] {
            assert!(checked_hook_url(bad).is_err(), "{bad}");
        }
    }

    // T-HOK-002
    #[test]
    fn messages_are_plain_or_json() {
        assert_eq!(line_of("  今日は\n"), Some("今日は".into()));
        assert_eq!(line_of(r#"{"sentence":"今日は"}"#), Some("今日は".into()));
        assert_eq!(line_of(r#"{"text":"今日は"}"#), Some("今日は".into()));
        assert_eq!(line_of(r#"{"other":1}"#), None);
        // braces that are not JSON are a line
        assert_eq!(line_of("{今日は"), Some("{今日は".into()));
        assert_eq!(line_of(" \n\t"), None);
        assert_eq!(line_of("a\u{7}b\u{202e}c"), Some("ab\u{202e}c".into()));
        assert_eq!(
            line_of(&"字".repeat(5000)).unwrap().chars().count(),
            LINE_LIMIT
        );
    }

    // T-HOK-003
    #[test]
    fn the_clipboard_keeps_only_japanese() {
        assert!(is_japanese("password1 です"));
        assert!(is_japanese("カタカナ") && is_japanese("漢字"));
        assert!(!is_japanese("password1") && !is_japanese("https://example.org"));
    }
}
