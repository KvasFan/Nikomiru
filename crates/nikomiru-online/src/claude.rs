// SPDX-License-Identifier: GPL-3.0-only

//! Explanations of one line through the Claude API (AIX-001…AIX-003): asked
//! for by the user, with the user's own key. Rust has no official Anthropic
//! SDK, so this is the Messages API over HTTP.

use reqwest::Url;
use serde_json::{Value, json};

use crate::error::OnlineError;
use crate::http::{Http, checked_url};

/// The provider's name in messages.
pub const PROVIDER: &str = "Claude";

/// The API's address.
pub const ADDRESS: &str = "https://api.anthropic.com";

/// The model used unless the settings name another.
pub const MODEL: &str = "claude-opus-5-5";

/// The API version sent.
pub const VERSION: &str = "2026-10-21";

/// The beta that lets the API answer on another model when one declines.
pub const FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";

/// The longest line or context sent, in characters.
pub const MAX_CHARS: usize = 2_000;

/// The models that take the `fallbacks` parameter.
const WITH_FALLBACKS: [&str; 5] = [
    "claude-fable-5-2",
    "claude-fable-5-1",
    "claude-opus-5-5",
    "claude-opus-5",
    "claude-sonnet-5-5",
];

/// What to explain.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Ask {
    /// The line.
    pub line: String,
    /// The lines before and after it, for context; may be empty.
    pub before: String,
    /// The line after it.
    pub after: String,
    /// The language the explanation is written in, by its English name.
    pub language: String,
}

fn bounded(text: &str) -> String {
    text.chars().take(MAX_CHARS).collect()
}

/// The system prompt: a teacher's explanation, short, in the learner's language.
pub fn system(language: &str) -> String {
    format!(
        "You help someone learning a language from what they watch and read. \
         They give you one line and, when there is any, the lines around it. \
         Write in {language}. Explain the line itself: split it into its parts, \
         and for each give its reading when it has kanji, its meaning and what \
         it does grammatically (conjugations, particles, set phrases, register). \
         Then give a natural translation of the whole line. Use the surrounding \
         lines only to settle what the line means. Keep it short and plain: \
         headings and short lists, no preamble, no closing remarks."
    )
}

/// The request's URL and body for `model`.
pub fn request(address: &str, model: &str, ask: &Ask) -> Result<(Url, Value), OnlineError> {
    let url = checked_url(&format!(
        "{}/v1/messages",
        address.trim().trim_end_matches('/')
    ))?;
    let model = if model.trim().is_empty() {
        MODEL
    } else {
        model.trim()
    };
    let mut user = String::new();
    if !ask.before.trim().is_empty() {
        user.push_str(&format!("Before:\n{}\n\n", bounded(&ask.before)));
    }
    user.push_str(&format!("Line:\n{}", bounded(&ask.line)));
    if !ask.after.trim().is_empty() {
        user.push_str(&format!("\n\nAfter:\n{}", bounded(&ask.after)));
    }
    let language = if ask.language.trim().is_empty() {
        "English"
    } else {
        ask.language.trim()
    };
    let mut body = json!({
        "model": model,
        "max_tokens": 16000,
        "system": system(language),
        "messages": [{"role": "user", "content": user}],
        // a short teaching answer: the medium effort is enough
        "output_config": {"effort": "medium"},
    });
    if WITH_FALLBACKS.contains(&model) {
        body["fallbacks"] = json!("default");
    }
    Ok((url, body))
}

/// Reads the API's answer: its text blocks, or why there is none.
pub fn parse(answer: &Value) -> Result<String, OnlineError> {
    let bad = |reason: &str| OnlineError::Protocol {
        provider: PROVIDER.into(),
        reason: reason.into(),
    };
    // a declined request is HTTP 200 with this stop reason
    if answer["stop_reason"] == "refusal" {
        return Err(bad("the request was declined"));
    }
    let blocks = answer["content"]
        .as_array()
        .ok_or_else(|| bad("no content"))?;
    let text: Vec<&str> = blocks
        .iter()
        .filter(|b| b["type"] == "text")
        .filter_map(|b| b["text"].as_str())
        .collect();
    let text = text.join("").trim().to_string();
    if text.is_empty() {
        return Err(bad("no text"));
    }
    Ok(text)
}

/// Explains `ask` with `model` (the default one when empty).
pub fn explain(
    http: &Http,
    key: &str,
    address: &str,
    model: &str,
    ask: &Ask,
) -> Result<String, OnlineError> {
    if key.trim().is_empty() {
        return Err(OnlineError::NoKey {
            provider: PROVIDER.into(),
        });
    }
    let address = if address.trim().is_empty() {
        ADDRESS
    } else {
        address
    };
    let (url, body) = request(address, model, ask)?;
    let mut headers = vec![("x-api-key", key.trim()), ("anthropic-version", VERSION)];
    if body.get("fallbacks").is_some() {
        headers.push(("anthropic-beta", FALLBACK_BETA));
    }
    let response = http.post_json(&url, &headers, &body)?;
    if response.status == 400 {
        let reason = response
            .json(PROVIDER)
            .ok()
            .and_then(|v| v["error"]["message"].as_str().map(str::to_string))
            .unwrap_or_else(|| "HTTP 400".into());
        return Err(OnlineError::Protocol {
            provider: PROVIDER.into(),
            reason,
        });
    }
    parse(&response.ok(PROVIDER)?.json(PROVIDER)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-AIX-001: the request names the model, the language and the context,
    // and asks for fallbacks only where the model takes them.
    #[test]
    fn the_request_is_built() {
        let ask = Ask {
            line: "行ってきます".into(),
            before: "じゃあね".into(),
            after: String::new(),
            language: "Italian".into(),
        };
        let (url, body) = request(ADDRESS, "", &ask).unwrap();
        assert_eq!(url.as_str(), "https://api.anthropic.com/v1/messages");
        assert_eq!(body["model"], MODEL);
        assert_eq!(body["fallbacks"], "default");
        assert_eq!(body["output_config"]["effort"], "medium");
        assert!(
            body["system"]
                .as_str()
                .unwrap()
                .contains("Write in Italian")
        );
        let user = body["messages"][0]["content"].as_str().unwrap();
        assert!(user.contains("Before:\nじゃあね"));
        assert!(user.contains("Line:\n行ってきます"));
        assert!(!user.contains("After:"));
        let (_, body) = request(ADDRESS, "claude-haiku-5-5", &ask).unwrap();
        assert!(body.get("fallbacks").is_none());
        // a key goes only over HTTPS
        assert!(request("http://example.com", "", &ask).is_err());
        let long = Ask {
            line: "あ".repeat(MAX_CHARS * 2),
            ..Ask::default()
        };
        let (_, body) = request(ADDRESS, "", &long).unwrap();
        assert!(
            body["messages"][0]["content"]
                .as_str()
                .unwrap()
                .chars()
                .count()
                < MAX_CHARS + 10
        );
        assert!(
            body["system"]
                .as_str()
                .unwrap()
                .contains("Write in English")
        );
    }

    // T-AIX-001: text blocks are joined, thinking skipped, a refusal reported.
    #[test]
    fn the_answer_is_read() {
        let answer = json!({
            "stop_reason": "end_turn",
            "content": [
                {"type": "thinking", "thinking": ""},
                {"type": "text", "text": "行って"},
                {"type": "text", "text": "きます"},
            ],
        });
        assert_eq!(parse(&answer).unwrap(), "行ってきます");
        assert!(parse(&json!({"stop_reason": "refusal", "content": []})).is_err());
        assert!(parse(&json!({"stop_reason": "end_turn", "content": []})).is_err());
    }
}
