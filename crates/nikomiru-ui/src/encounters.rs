// SPDX-License-Identifier: GPL-3.0-only

//! Collect unknown headwords from active playback text only (KNW-018).
//! Segmentation ahead of playback and passive analyses never call this with
//! their text. The database deduplicates across visits and process restarts.

use crate::words::{WordStatus, Words};
use std::collections::HashSet;

/// Bound the current media visit's in-memory deduplication set.
const MAX_WORDS: usize = 50_000;

/// Current playback visit; stores only headword/reading keys, not subtitles.
#[derive(Debug, Default)]
pub(crate) struct Collector {
    uid: Option<String>,
    seen: HashSet<(String, String)>,
}

impl Collector {
    /// Newly encountered unknown words of active, already segmented text.
    /// A paused/pending line is ignored and can be counted when ready/playing.
    pub(crate) fn observe<'a>(
        &mut self,
        uid: &str,
        playing: bool,
        texts: impl IntoIterator<Item = &'a str>,
        words: &Words,
    ) -> Vec<(String, String)> {
        if self.uid.as_deref() != Some(uid) {
            self.uid = Some(uid.into());
            self.seen.clear();
        }
        if !playing {
            return Vec::new();
        }
        let mut fresh = Vec::new();
        for text in texts {
            let Some(split) = words.get(text) else {
                continue;
            };
            for headword in split.iter().filter_map(|w| w.headword.as_ref()) {
                if self.seen.len() >= MAX_WORDS {
                    return fresh;
                }
                if words.status(&headword.expression, &headword.reading) == WordStatus::Unknown {
                    let key = (headword.expression.clone(), headword.reading.clone());
                    if self.seen.insert(key.clone()) {
                        fresh.push(key);
                    }
                }
            }
        }
        fresh
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nikomiru_dict::segment::{Headword, Word};

    fn words() -> Words {
        let mut words = Words::default();
        for text in ["猫", "犬", "本", "読む"] {
            words.done(
                0,
                text.into(),
                Ok(vec![Word {
                    range: 0..text.len(),
                    headword: Some(Headword {
                        expression: text.into(),
                        reading: text.into(),
                        pitch: None,
                    }),
                }]),
            );
        }
        words
    }

    // T-KNW-018: active texts only, unknown status at the moment of playback,
    // repeated words once; paused/pending words remain eligible later.
    #[test]
    fn only_active_unknown_words_are_emitted_once_per_media_visit() {
        let mut collector = Collector::default();
        let mut words = words();
        words.set_status("犬", "犬", WordStatus::Known);
        words.set_status("本", "本", WordStatus::Learning);
        words.set_status("読む", "読む", WordStatus::Ignored);
        assert!(collector.observe("a", false, ["猫"], &words).is_empty());
        assert!(collector.observe("a", true, ["未分割"], &words).is_empty());
        // The cache also has prefetched words, but only active text is passed.
        assert!(
            collector
                .observe("a", true, ["犬", "本", "読む"], &words)
                .is_empty()
        );
        assert_eq!(
            collector.observe("a", true, ["猫", "猫"], &words),
            [("猫".into(), "猫".into())]
        );
        assert!(collector.observe("a", true, ["猫"], &words).is_empty());
        words.set_status("猫", "猫", WordStatus::Known);
        words.set_status("猫", "猫", WordStatus::Unknown);
        assert!(collector.observe("a", true, ["猫"], &words).is_empty());
        words.set_status("犬", "犬", WordStatus::Unknown);
        assert_eq!(collector.observe("a", true, ["犬"], &words).len(), 1);
        assert_eq!(collector.observe("b", true, ["猫"], &words).len(), 1);
    }

    // T-KNW-018: the playback visit is bounded; switching media releases it.
    #[test]
    fn the_visit_deduplication_set_is_bounded() {
        let mut collector = Collector {
            uid: Some("a".into()),
            seen: (0..MAX_WORDS)
                .map(|i| (i.to_string(), String::new()))
                .collect(),
        };
        let words = words();
        assert!(collector.observe("a", true, ["猫"], &words).is_empty());
        assert_eq!(collector.seen.len(), MAX_WORDS);
        assert_eq!(collector.observe("b", true, ["猫"], &words).len(), 1);
        assert_eq!(collector.seen.len(), 1);
    }
}
