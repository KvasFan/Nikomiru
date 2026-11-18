// SPDX-License-Identifier: GPL-3.0-only

//! Machine translation of one line through DeepL's API (TRN-001…TRN-003):
//! asked for by the user, one line at a time, with the user's own key.

use reqwest::Url;
use serde_json::{Value, json};

use crate::error::OnlineError;
use crate::http::{Http, checked_url};

/// The provider's name in messages.
pub const PROVIDER: &str = "DeepL";

/// DeepL API Free: its keys end in `:fx`.
pub const FREE_ADDRESS: &str = "https://api-free.deepl.com";

/// DeepL API Pro.
pub const PRO_ADDRESS: &str = "https://api.deepl.com";

/// The longest line sent, in characters: a subtitle or a sentence, never a
/// whole text.
pub const MAX_CHARS: usize = 2_000;

/// The address a key belongs to, unless the settings name another one.
pub fn address_for(key: &str, configured: &str) -> String {
    if !configured.trim().is_empty() {
        configured.trim().trim_end_matches('/').to_string()
    } else if key.trim().ends_with(":fx") {
        FREE_ADDRESS.into()
    } else {
        PRO_ADDRESS.into()
    }
}

/// DeepL's target language for an interface language (`en`, `it`, `ja`…):
/// English and Portuguese need their variant.
pub fn target_for(language: &str) -> String {
    let lower = language.trim().to_ascii_lowercase();
    let base = lower.split(['-', '_']).next().unwrap_or_default();
    match base {
        "" | "en" => "EN-US".into(),
        "pt" => "PT-BR".into(),
        "zh" => "ZH-HANS".into(),
        other => other.to_ascii_uppercase(),
    }
}

/// What DeepL answered: the translation and the language it detected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Translation {
    /// The translated line.
    pub text: String,
    /// The source language DeepL detected (`JA`).
    pub detected: String,
}

/// The request's URL and body.
pub fn request(address: &str, text: &str, target: &str) -> Result<(Url, Value), OnlineError> {
    let url = checked_url(&format!(
        "{}/v2/translate",
        address.trim().trim_end_matches('/')
    ))?;
    let text: String = text.chars().take(MAX_CHARS).collect();
    Ok((
        url,
        json!({
            "text": [text],
            "target_lang": target,
            // subtitles are short: no guessing of the source from the target's
            // neighbours, and line breaks kept as they are
            "preserve_formatting": true,
        }),
    ))
}

/// Reads DeepL's answer.
pub fn parse(answer: &Value) -> Result<Translation, OnlineError> {
    let bad = |reason: &str| OnlineError::Protocol {
        provider: PROVIDER.into(),
        reason: reason.into(),
    };
    let first = answer["translations"]
        .as_array()
        .and_then(|t| t.first())
        .ok_or_else(|| bad("no translation"))?;
    let text = first["text"].as_str().ok_or_else(|| bad("no text"))?;
    Ok(Translation {
        text: text.to_string(),
        detected: first["detected_source_language"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
    })
}

/// Translates `text` into `target`.
pub fn translate(
    http: &Http,
    key: &str,
    address: &str,
    text: &str,
    target: &str,
) -> Result<Translation, OnlineError> {
    if key.trim().is_empty() {
        return Err(OnlineError::NoKey {
            provider: PROVIDER.into(),
        });
    }
    let (url, body) = request(&address_for(key, address), text, target)?;
    let auth = format!("DeepL-Auth-Key {}", key.trim());
    let response = http.post_json(&url, &[("Authorization", &auth)], &body)?;
    // 456: the month's characters are used up
    if response.status == 456 {
        return Err(OnlineError::Quota {
            provider: PROVIDER.into(),
        });
    }
    parse(&response.ok(PROVIDER)?.json(PROVIDER)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-TRN-001: the key chooses the API, the interface language the target.
    #[test]
    fn the_key_and_the_language_choose_the_request() {
        assert_eq!(address_for("abc:fx", ""), FREE_ADDRESS);
        assert_eq!(address_for("abc", ""), PRO_ADDRESS);
        assert_eq!(
            address_for("abc", "http://127.0.0.1:9/deepl/"),
            "http://127.0.0.1:9/deepl"
        );
        assert_eq!(target_for("en"), "EN-US");
        assert_eq!(target_for("it"), "IT");
        assert_eq!(target_for("pt-PT"), "PT-BR");
        assert_eq!(target_for(""), "EN-US");
        let long = "あ".repeat(MAX_CHARS + 50);
        let (url, body) = request("https://api-free.deepl.com", &long, "IT").unwrap();
        assert_eq!(url.as_str(), "https://api-free.deepl.com/v2/translate");
        assert_eq!(body["target_lang"], "IT");
        assert_eq!(body["text"][0].as_str().unwrap().chars().count(), MAX_CHARS);
        // a key goes only over HTTPS
        assert!(request("http://example.com", "x", "IT").is_err());
    }

    // T-TRN-001: the answer is read; a strange one is an error.
    #[test]
    fn the_answer_is_read() {
        let t =
            parse(&json!({"translations": [{"detected_source_language": "JA", "text": "Ciao"}]}))
                .unwrap();
        assert_eq!(t.text, "Ciao");
        assert_eq!(t.detected, "JA");
        assert!(parse(&json!({"translations": []})).is_err());
        assert!(parse(&json!({})).is_err());
    }
}
