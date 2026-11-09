// SPDX-License-Identifier: GPL-3.0-only

//! The words the learner knows, from the cards of their Anki decks
//! (KNW-005), as asbplayer and Migaku take them: a word whose card has an
//! interval of at least [`KNOWN_DAYS`] is known, one whose card was studied
//! is being learned; new and suspended cards say nothing.

use std::collections::HashMap;

use serde_json::{Value, json};

use crate::client::{Client, deck_query};
use crate::error::AnkiError;

/// The interval, in days, from which a card's word is known (Anki's
/// "mature").
pub const KNOWN_DAYS: i64 = 21;

/// The cards asked about in one `cardsInfo`.
const BATCH: usize = 500;

/// The most cards read: a large collection has tens of thousands.
pub const MAX_CARDS: usize = 200_000;

/// What a card says of its word.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Known {
    /// Studied, not mature yet.
    Learning,
    /// Mature.
    Known,
}

/// The word of a card's field: its text without HTML, with the readings of
/// `{furigana-plain}` (`漢字[かんじ]`) left out; `None` when empty.
pub fn word_of(field: &str) -> Option<String> {
    let text = crate::html::text(field);
    let mut out = String::with_capacity(text.len());
    let mut in_reading = false;
    for c in text.chars() {
        match c {
            '[' => in_reading = true,
            ']' => in_reading = false,
            _ if in_reading => {}
            c if c.is_whitespace() => {}
            c => out.push(c),
        }
    }
    (!out.is_empty()).then_some(out)
}

/// What one card says, from its `cardsInfo`: `None` for a new or suspended
/// card.
fn says(card: &Value) -> Option<Known> {
    let queue = card["queue"].as_i64().unwrap_or(0);
    let kind = card["type"].as_i64().unwrap_or(0);
    if queue < 0 || kind == 0 {
        return None;
    }
    Some(if card["interval"].as_i64().unwrap_or(0) >= KNOWN_DAYS {
        Known::Known
    } else {
        Known::Learning
    })
}

impl Client {
    /// The words of the cards of `decks`, read from their field `field`,
    /// each with the most its cards say (KNW-005).
    pub fn known_words(
        &self,
        decks: &[String],
        field: &str,
    ) -> Result<HashMap<String, Known>, AnkiError> {
        if decks.is_empty() {
            return Ok(HashMap::new());
        }
        let decks: Vec<String> = decks.iter().map(|d| deck_query(d)).collect();
        let query = format!("({})", decks.join(" OR "));
        let found = self.call("findCards", Some(json!({ "query": query })))?;
        let Value::Array(ids) = found else {
            return Err(self.protocol("findCards is not a list"));
        };
        let ids: Vec<u64> = ids
            .iter()
            .filter_map(Value::as_u64)
            .take(MAX_CARDS)
            .collect();
        let mut out: HashMap<String, Known> = HashMap::new();
        for batch in ids.chunks(BATCH) {
            let Value::Array(cards) = self.call("cardsInfo", Some(json!({ "cards": batch })))?
            else {
                return Err(self.protocol("cardsInfo is not a list"));
            };
            for card in &cards {
                let (Some(known), Some(word)) = (
                    says(card),
                    card["fields"][field]["value"].as_str().and_then(word_of),
                ) else {
                    continue;
                };
                let best = out.entry(word).or_insert(known);
                *best = (*best).max(known);
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-KNW-005: the word of a field, without its HTML and readings
    #[test]
    fn the_word_of_a_field() {
        assert_eq!(word_of("<b>食べる</b>"), Some("食べる".into()));
        assert_eq!(word_of(" 食[た]べ 物[もの]"), Some("食べ物".into()));
        assert_eq!(word_of("&nbsp;"), None);
        assert_eq!(word_of(""), None);
    }

    // T-KNW-005: mature known, studied learning, new and suspended nothing
    #[test]
    fn what_a_card_says() {
        let card =
            |interval, kind, queue| json!({ "interval": interval, "type": kind, "queue": queue });
        assert_eq!(says(&card(30, 2, 2)), Some(Known::Known));
        assert_eq!(says(&card(21, 2, 2)), Some(Known::Known));
        assert_eq!(says(&card(3, 2, 2)), Some(Known::Learning));
        assert_eq!(says(&card(0, 1, 1)), Some(Known::Learning));
        assert_eq!(says(&card(0, 0, 0)), None, "new");
        assert_eq!(says(&card(40, 2, -1)), None, "suspended");
    }
}
