// SPDX-License-Identifier: GPL-3.0-only

//! L4 tests of a line translated by DeepL (TRN-001) and explained by Claude
//! (AIX-001), against fake APIs.

#![cfg(feature = "test-support")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use nikomiru_online::claude::{self, Ask};
use nikomiru_online::deepl;
use nikomiru_online::error::OnlineError;
use nikomiru_online::fake::FakeOnline;
use nikomiru_online::http::Http;
use serde_json::Value;

// T-TRN-001: the key goes in DeepL's header, the line and target in the
// body; a wrong key and a used-up quota are told apart.
#[test]
fn deepl_translates_a_line() {
    let fake = FakeOnline::start().unwrap();
    fake.data()
        .translations
        .insert("行ってきます".into(), "I'm off".into());
    let http = Http::new("").unwrap();
    let t = deepl::translate(
        &http,
        "deepl-key:fx",
        &fake.deepl_url(),
        "行ってきます",
        "EN-US",
    )
    .unwrap();
    assert_eq!(t.text, "I'm off");
    assert_eq!(t.detected, "JA");
    let requests = fake.requests();
    let r = requests.last().unwrap();
    assert_eq!(r.target, "/deepl/v2/translate");
    assert_eq!(
        r.header("authorization"),
        Some("DeepL-Auth-Key deepl-key:fx")
    );
    let body: Value = serde_json::from_str(&r.body).unwrap();
    assert_eq!(body["target_lang"], "EN-US");
    assert_eq!(body["text"][0], "行ってきます");
    assert!(matches!(
        deepl::translate(&http, "other", &fake.deepl_url(), "行ってきます", "EN-US"),
        Err(OnlineError::Unauthorized { .. })
    ));
    assert!(matches!(
        deepl::translate(&http, "deepl-key:fx", &fake.deepl_url(), "未知", "EN-US"),
        Err(OnlineError::Quota { .. })
    ));
    // no key: nothing is sent
    fake.clear_requests();
    assert!(matches!(
        deepl::translate(&http, " ", &fake.deepl_url(), "x", "EN-US"),
        Err(OnlineError::NoKey { .. })
    ));
    assert!(fake.requests().is_empty());
}

// T-AIX-001: the Messages API gets the key, the version, the model and the
// line with its context; a refused key and a missing key are reported.
#[test]
fn claude_explains_a_line() {
    let fake = FakeOnline::start().unwrap();
    fake.data().explanation = "行って (te-form of 行く) + きます".into();
    let http = Http::new("").unwrap();
    let ask = Ask {
        line: "行ってきます".into(),
        before: "じゃあね".into(),
        after: "いってらっしゃい".into(),
        language: "Italian".into(),
    };
    let text = claude::explain(&http, "claude-key", &fake.claude_url(), "", &ask).unwrap();
    assert_eq!(text, "行って (te-form of 行く) + きます");
    let requests = fake.requests();
    let r = requests.last().unwrap();
    assert_eq!(r.target, "/claude/v1/messages");
    assert_eq!(r.header("x-api-key"), Some("claude-key"));
    assert_eq!(r.header("anthropic-version"), Some(claude::VERSION));
    assert_eq!(r.header("anthropic-beta"), Some(claude::FALLBACK_BETA));
    let body: Value = serde_json::from_str(&r.body).unwrap();
    assert_eq!(body["model"], claude::MODEL);
    let user = body["messages"][0]["content"].as_str().unwrap();
    assert!(
        user.contains("じゃあね")
            && user.contains("行ってきます")
            && user.contains("いってらっしゃい")
    );
    // another model without fallbacks: no beta header
    claude::explain(
        &http,
        "claude-key",
        &fake.claude_url(),
        "claude-haiku-5-5",
        &ask,
    )
    .unwrap();
    let requests = fake.requests();
    assert_eq!(requests.last().unwrap().header("anthropic-beta"), None);
    assert!(matches!(
        claude::explain(&http, "wrong", &fake.claude_url(), "", &ask),
        Err(OnlineError::Unauthorized { .. })
    ));
    fake.clear_requests();
    assert!(matches!(
        claude::explain(&http, "", &fake.claude_url(), "", &ask),
        Err(OnlineError::NoKey { .. })
    ));
    assert!(fake.requests().is_empty());
}
