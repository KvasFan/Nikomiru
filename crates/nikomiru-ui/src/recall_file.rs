// SPDX-License-Identifier: GPL-3.0-only

//! Bounded replay-validated portable recall progress (KNW-059).
use crate::recall::{Card, Grade, Recall};
use serde::{
    Deserialize, Deserializer, Serialize,
    de::{Error, SeqAccess, Visitor},
};
use std::{io::Write, marker::PhantomData};
pub(crate) const MAX_BYTES: usize = 16 << 20;
fn bounded<'de, D, T, const N: usize>(d: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct V<T, const N: usize>(PhantomData<T>);
    impl<'de, T: Deserialize<'de>, const N: usize> Visitor<'de> for V<T, N> {
        type Value = Vec<T>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            write!(f, "at most {N} entries")
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            let mut out = Vec::new();
            while let Some(value) = seq.next_element()? {
                if out.len() >= N {
                    return Err(A::Error::custom("recall entry limit"));
                }
                out.push(value);
            }
            Ok(out)
        }
    }
    d.deserialize_seq(V::<T, N>(PhantomData))
}
pub(crate) fn hints<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    bounded::<D, String, 3>(d)
}
fn cards<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Card>, D::Error> {
    bounded::<D, Card, 1000>(d)
}
fn order<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<usize>, D::Error> {
    bounded::<D, usize, 1000>(d)
}
fn history<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<(usize, Grade)>, D::Error> {
    bounded::<D, (usize, Grade), 3000>(d)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    format: u32,
    round: String,
    source: String,
    #[serde(deserialize_with = "cards")]
    cards: Vec<Card>,
    #[serde(deserialize_with = "order")]
    order: Vec<usize>,
    #[serde(default, deserialize_with = "history")]
    history: Vec<(usize, Grade)>,
    #[serde(default)]
    hint_source: Option<String>,
}
#[derive(Serialize)]
struct Save<'a> {
    format: u32,
    round: &'a str,
    source: &'a str,
    cards: &'a [Card],
    order: &'a [usize],
    history: &'a [(usize, Grade)],
    hint_source: &'a Option<String>,
}
struct Limited(Vec<u8>);
impl Write for Limited {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.0.len().saturating_add(bytes.len()) > MAX_BYTES {
            return Err(std::io::Error::other("recall size limit"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
pub(crate) fn save(r: &Recall) -> Result<Vec<u8>, &'static str> {
    let file = Save {
        format: 1,
        round: &r.id,
        source: &r.source,
        cards: &r.cards,
        order: &r.order,
        history: &r.history,
        hint_source: &r.hint_source,
    };
    let mut out = Limited(Vec::new());
    serde_json::to_writer_pretty(&mut out, &file).map_err(|_| "recall-file-too-large")?;
    Ok(out.0)
}
pub(crate) fn load(bytes: &[u8]) -> Result<Recall, &'static str> {
    if bytes.len() > MAX_BYTES {
        return Err("recall-file-too-large");
    }
    let file: Snapshot = serde_json::from_slice(bytes).map_err(|_| "recall-file-invalid")?;
    if file.format != 1 {
        return Err("recall-file-version");
    }
    let title_valid = |s: &str| s.chars().count() <= 256 && !s.chars().any(char::is_control);
    let id = uuid::Uuid::parse_str(&file.round).map_err(|_| "recall-file-invalid")?;
    if file.round.len() != 36
        || !title_valid(&file.source)
        || file.hint_source.as_ref().is_some_and(|s| !title_valid(s))
    {
        return Err("recall-file-invalid");
    }
    let mut r = Recall::from_cards(&file.source, file.cards)?;
    let unique = file
        .order
        .iter()
        .copied()
        .collect::<std::collections::HashSet<_>>();
    if file.order.len() != r.cards.len()
        || unique.len() != r.cards.len()
        || file.order.iter().any(|i| *i >= r.cards.len())
        || file.history.len() > r.cards.len() * 3
    {
        return Err("recall-file-invalid");
    }
    r.order = file.order;
    r.pending = r.order.iter().copied().collect();
    r.id = id.to_string();
    r.hint_source = file.hint_source;
    for (index, grade) in file.history {
        if r.pending.front() != Some(&index) {
            return Err("recall-file-invalid");
        }
        if grade == Grade::Skipped {
            r.skip();
        } else {
            r.reveal();
            if !r.rate(grade == Grade::Good) {
                return Err("recall-file-invalid");
            }
        }
    }
    r.revealed = false;
    r.hint_revealed = false;
    Ok(r)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::words::{ListedWord, WordStatus};
    fn round() -> Recall {
        Recall::start(
            "Study",
            &[
                ListedWord {
                    expression: "猫".into(),
                    reading: "ねこ".into(),
                    count: 2,
                    status: WordStatus::Known,
                    card: false,
                },
                ListedWord {
                    expression: "犬".into(),
                    reading: "いぬ".into(),
                    count: 1,
                    status: WordStatus::Unknown,
                    card: false,
                },
            ],
        )
        .unwrap()
    }
    // T-KNW-059: a frozen session restores progress by replay, keeps its identity and hides the current answer.
    #[test]
    fn recall_file_round_trips_only_captured_progress() {
        let mut r = round();
        r.reveal();
        r.rate(false);
        r.reveal();
        let bytes = save(&r).unwrap();
        let read = load(&bytes).unwrap();
        assert_eq!(read.id, r.id);
        assert_eq!(read.pending, r.pending);
        assert_eq!(read.history, r.history);
        assert!(!read.revealed);
        assert_eq!(read.cards, r.cards);
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(value.get("statuses").is_none());
        assert!(value.get("path").is_none());
    }
    // T-KNW-059: malformed orders, history and nested oversized arrays reject without a partial round.
    #[test]
    fn recall_file_rejects_tampered_and_oversized_progress() {
        let good: serde_json::Value = serde_json::from_slice(&save(&round()).unwrap()).unwrap();
        for (key, value) in [
            ("order", serde_json::json!([0, 0])),
            ("history", serde_json::json!([[1, "good"]])),
            ("format", serde_json::json!(2)),
            ("extra", serde_json::json!(true)),
            ("round", serde_json::json!("invalid")),
        ] {
            let mut bad = good.clone();
            bad[key] = value;
            assert!(load(&serde_json::to_vec(&bad).unwrap()).is_err());
        }
        let mut bad = good;
        bad["cards"][0]["hints"] = serde_json::json!(["a", "b", "c", "d"]);
        assert!(load(&serde_json::to_vec(&bad).unwrap()).is_err());
        assert!(load(&vec![b' '; MAX_BYTES + 1]).is_err());
    }
}
