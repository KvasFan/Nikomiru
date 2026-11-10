// SPDX-License-Identifier: GPL-3.0-only

//! What the learner knows of each word (KNW-001), in the `word_status`
//! table of `library.redb`, by headword and reading. A word never marked
//! has no row; a word marked back to unknown keeps one, with its time, so
//! that the newest change wins when devices sync (BAK-005). A new table
//! leaves the other records as they are, so the schema version does not
//! change (ADR 0007 §26).

use std::collections::HashMap;

use redb::{ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};

use crate::db::{Library, LibraryError, db_err};

pub(crate) const WORD_STATUS: TableDefinition<&str, &[u8]> = TableDefinition::new("word_status");

/// The words the cards of Anki's decks say are known or being learned, by
/// headword (KNW-005): read again from Anki, so neither backed up nor
/// synced; a status set by hand wins over them.
const ANKI_WORDS: TableDefinition<&str, &[u8]> = TableDefinition::new("anki_words");

/// What the learner knows of a word.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum WordStatus {
    /// Not known yet.
    Unknown,
    /// Being learned: a card was made, or the learner said so.
    Learning,
    /// Known.
    Known,
    /// Left out of the counts: a name, a sound, noise.
    Ignored,
}

/// A word's status and when it was set (Unix ms).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WordMark {
    /// The status.
    pub status: WordStatus,
    /// When it was set.
    pub changed_at: i64,
}

impl WordMark {
    /// Which of two marks of a word wins: the newer, then, so that every
    /// device picks the same, the greater status.
    pub fn wins_over(&self, other: &WordMark) -> bool {
        (self.changed_at, self.status) > (other.changed_at, other.status)
    }
}

/// The key of a word: its headword and its reading, as the popup groups
/// them (LKP-036).
pub fn word_key(expression: &str, reading: &str) -> String {
    format!("{expression}\n{reading}")
}

/// The largest word list read (KNW-006).
pub const MAX_LIST_BYTES: u64 = 16 << 20;

impl WordStatus {
    /// Its name in a word list.
    pub fn name(self) -> &'static str {
        match self {
            WordStatus::Unknown => "unknown",
            WordStatus::Learning => "learning",
            WordStatus::Known => "known",
            WordStatus::Ignored => "ignored",
        }
    }

    /// A status from its name, in any case.
    pub fn from_name(name: &str) -> Option<Self> {
        match name.trim().to_lowercase().as_str() {
            "unknown" | "new" => Some(WordStatus::Unknown),
            "learning" => Some(WordStatus::Learning),
            "known" => Some(WordStatus::Known),
            "ignored" | "ignore" => Some(WordStatus::Ignored),
            _ => None,
        }
    }
}

/// The words of a list (KNW-006): a line is `word`, known with any
/// reading, or `word,reading,status` (a comma or a tab between); a heading
/// line and empty lines are skipped, and so is a line whose status is not
/// one of the four.
pub fn parse_list(text: &str) -> Vec<(String, String, WordStatus)> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim().trim_start_matches('\u{feff}');
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.split([',', '\t']).map(str::trim).collect();
        let (word, reading, status) = match parts.as_slice() {
            [word] => (*word, "", Some(WordStatus::Known)),
            [word, reading] => (*word, *reading, Some(WordStatus::Known)),
            [word, reading, status, ..] => (*word, *reading, WordStatus::from_name(status)),
            [] => continue,
        };
        if word.is_empty() || word.eq_ignore_ascii_case("expression") || word == "word" {
            continue;
        }
        if let Some(status) = status {
            out.push((word.to_string(), reading.to_string(), status));
        }
    }
    out
}

/// The words marked as a list that [`parse_list`] reads back: a heading,
/// then `word,reading,status`, by word.
pub fn write_list(marks: &HashMap<String, WordMark>) -> String {
    let mut rows: Vec<(&str, &str, WordStatus)> = marks
        .iter()
        .filter_map(|(key, mark)| {
            let (word, reading) = key.split_once('\n')?;
            Some((word, reading, mark.status))
        })
        .collect();
    rows.sort();
    let mut out = String::from("expression,reading,status\n");
    for (word, reading, status) in rows {
        // a comma would cut the line: such a word is not a word
        if word.contains([',', '\t']) || reading.contains([',', '\t']) {
            continue;
        }
        out.push_str(&format!("{word},{reading},{}\n", status.name()));
    }
    out
}

/// How much of a subtitle the learner understands (KNW-007).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Score {
    /// The words met, each time it is met: those a dictionary knows, not
    /// ignored.
    pub words: usize,
    /// Of them, the known ones.
    pub known: usize,
    /// The lines with at least one such word.
    pub lines: usize,
    /// Of them, the lines with no word not known.
    pub clear: usize,
}

impl Score {
    /// Counts a line, given the statuses of its words in order.
    pub fn add_line(&mut self, statuses: impl IntoIterator<Item = WordStatus>) {
        let (mut counted, mut clear) = (false, true);
        for status in statuses {
            match status {
                WordStatus::Ignored => continue,
                WordStatus::Known => self.known += 1,
                WordStatus::Learning => {}
                WordStatus::Unknown => clear = false,
            }
            self.words += 1;
            counted = true;
        }
        if counted {
            self.lines += 1;
            self.clear += usize::from(clear);
        }
    }

    /// The share of the words known, in whole percent (rounded down, so
    /// that 100 means every one).
    pub fn known_percent(&self) -> Option<usize> {
        (self.words > 0).then(|| self.known * 100 / self.words)
    }

    /// The share of the lines with no word not known, in whole percent.
    pub fn clear_percent(&self) -> Option<usize> {
        (self.lines > 0).then(|| self.clear * 100 / self.lines)
    }
}

/// The words of a subtitle's lines, counted once so that its score can be
/// worked out again whenever the statuses change (KNW-008).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Counted {
    /// Each word met, by headword and reading, once.
    pub words: Vec<(String, String)>,
    /// The lines with a word, as the indices of their words in `words`, in
    /// order and with repeats.
    pub lines: Vec<Vec<u32>>,
}

impl Counted {
    /// The words of each line, by headword and reading.
    pub fn from_lines<I, L>(lines: I) -> Self
    where
        I: IntoIterator<Item = L>,
        L: IntoIterator<Item = (String, String)>,
    {
        let mut counted = Counted::default();
        let mut index: HashMap<(String, String), u32> = HashMap::new();
        for line in lines {
            let line: Vec<u32> = line
                .into_iter()
                .map(|word| {
                    *index.entry(word.clone()).or_insert_with(|| {
                        counted.words.push(word);
                        (counted.words.len() - 1) as u32
                    })
                })
                .collect();
            if !line.is_empty() {
                counted.lines.push(line);
            }
        }
        counted
    }

    /// How many times each word of `words` comes (KNW-009).
    pub fn occurrences(&self) -> Vec<usize> {
        let mut out = vec![0; self.words.len()];
        for &i in self.lines.iter().flatten() {
            if let Some(n) = out.get_mut(i as usize) {
                *n += 1;
            }
        }
        out
    }

    /// The score, given what the learner knows of each word; each word is
    /// asked once.
    pub fn score(&self, status: impl Fn(&str, &str) -> WordStatus) -> Score {
        let statuses: Vec<WordStatus> = self.words.iter().map(|(e, r)| status(e, r)).collect();
        let mut score = Score::default();
        for line in &self.lines {
            score.add_line(
                line.iter()
                    .filter_map(|&i| statuses.get(i as usize).copied()),
            );
        }
        score
    }
}

/// The days the word statistics chart (KNW-010).
pub const WORD_DAYS: i64 = 30;

/// What the Statistics tab says of the words (KNW-010).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WordStats {
    /// The words set known by hand or by a list.
    pub known: usize,
    /// Set being learned.
    pub learning: usize,
    /// Set ignored.
    pub ignored: usize,
    /// The words Anki's cards say are known (KNW-005).
    pub anki_known: usize,
    /// Those they say are being learned.
    pub anki_learning: usize,
    /// The words known now that were set known by the end of each of the
    /// last [`WORD_DAYS`] days, today last.
    pub known_by_day: Vec<u32>,
}

impl WordStats {
    /// The statistics of the statuses set and of Anki's words, on local
    /// day `today` (the review's days, with its rollover).
    pub fn of(
        marks: &HashMap<String, WordMark>,
        anki: &HashMap<String, WordStatus>,
        today: i64,
        offset_s: i32,
    ) -> Self {
        let mut s = WordStats {
            known_by_day: vec![0; WORD_DAYS as usize],
            ..WordStats::default()
        };
        for mark in marks.values() {
            match mark.status {
                WordStatus::Unknown => {}
                WordStatus::Learning => s.learning += 1,
                WordStatus::Ignored => s.ignored += 1,
                WordStatus::Known => {
                    s.known += 1;
                    let day = crate::review::day_of(mark.changed_at, offset_s);
                    let first = today - (WORD_DAYS - 1);
                    // known from that day on: every bar from it counts it
                    let from = (day - first).max(0);
                    for n in s.known_by_day.iter_mut().skip(from as usize) {
                        *n += 1;
                    }
                }
            }
        }
        for status in anki.values() {
            match status {
                WordStatus::Known => s.anki_known += 1,
                WordStatus::Learning => s.anki_learning += 1,
                _ => {}
            }
        }
        s
    }

    /// Whether there is anything to show.
    pub fn any(&self) -> bool {
        self.known + self.learning + self.ignored + self.anki_known + self.anki_learning > 0
    }
}

impl Library {
    /// Sets the status of a word.
    pub fn set_word_status(
        &self,
        expression: &str,
        reading: &str,
        mark: WordMark,
    ) -> Result<(), LibraryError> {
        let bytes = postcard::to_stdvec(&mark)?;
        let tx = self.write()?;
        {
            let mut t = db_err!(tx.open_table(WORD_STATUS))?;
            db_err!(t.insert(word_key(expression, reading).as_str(), bytes.as_slice()))?;
        }
        db_err!(tx.commit())
    }

    /// Sets the statuses of many words at once (KNW-006): one transaction.
    pub fn set_word_statuses(
        &self,
        marks: &[(String, String, WordMark)],
    ) -> Result<(), LibraryError> {
        let tx = self.write()?;
        {
            let mut t = db_err!(tx.open_table(WORD_STATUS))?;
            for (expression, reading, mark) in marks {
                let bytes = postcard::to_stdvec(mark)?;
                db_err!(t.insert(word_key(expression, reading).as_str(), bytes.as_slice()))?;
            }
        }
        db_err!(tx.commit())
    }

    /// Replaces the words read from Anki (KNW-005).
    pub fn set_anki_words(&self, words: &HashMap<String, WordStatus>) -> Result<(), LibraryError> {
        let tx = self.write()?;
        {
            let _ = tx.delete_table(ANKI_WORDS);
            let mut t = db_err!(tx.open_table(ANKI_WORDS))?;
            for (word, status) in words {
                let bytes = postcard::to_stdvec(status)?;
                db_err!(t.insert(word.as_str(), bytes.as_slice()))?;
            }
        }
        db_err!(tx.commit())
    }

    /// The words last read from Anki (KNW-005).
    pub fn anki_words(&self) -> Result<HashMap<String, WordStatus>, LibraryError> {
        self.read(|tx| match tx.open_table(ANKI_WORDS) {
            Ok(t) => {
                let mut out = HashMap::new();
                for row in db_err!(t.iter())? {
                    let (k, v) = db_err!(row)?;
                    out.insert(k.value().to_string(), postcard::from_bytes(v.value())?);
                }
                Ok(out)
            }
            Err(redb::TableError::TableDoesNotExist(_)) => Ok(HashMap::new()),
            Err(e) => Err(LibraryError::Db(e.into())),
        })
    }

    /// The word statistics on local day `today` (KNW-010).
    pub fn word_stats(&self, today: i64, offset_s: i32) -> Result<WordStats, LibraryError> {
        Ok(WordStats::of(
            &self.word_statuses()?,
            &self.anki_words()?,
            today,
            offset_s,
        ))
    }

    /// Every word marked, by [`word_key`].
    pub fn word_statuses(&self) -> Result<HashMap<String, WordMark>, LibraryError> {
        self.read(|tx| match tx.open_table(WORD_STATUS) {
            Ok(t) => {
                let mut out = HashMap::new();
                for row in db_err!(t.iter())? {
                    let (k, v) = db_err!(row)?;
                    out.insert(k.value().to_string(), postcard::from_bytes(v.value())?);
                }
                Ok(out)
            }
            Err(redb::TableError::TableDoesNotExist(_)) => Ok(HashMap::new()),
            Err(e) => Err(LibraryError::Db(e.into())),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-KNW-010: the words by status; the ones known now, by the day they
    // were set known, counted on that day and after
    #[test]
    fn the_word_statistics_count_by_status_and_day() {
        let day_ms = 86_400_000;
        // noon of day 100, in UTC
        let at = |day: i64| day * day_ms + day_ms / 2;
        let mark = |status, day| WordMark {
            status,
            changed_at: at(day),
        };
        let marks = HashMap::from([
            ("a\n".to_string(), mark(WordStatus::Known, 50)),
            ("b\n".to_string(), mark(WordStatus::Known, 99)),
            ("c\n".to_string(), mark(WordStatus::Known, 100)),
            ("d\n".to_string(), mark(WordStatus::Learning, 100)),
            ("e\n".to_string(), mark(WordStatus::Ignored, 100)),
            ("f\n".to_string(), mark(WordStatus::Unknown, 100)),
        ]);
        let anki = HashMap::from([
            ("g".to_string(), WordStatus::Known),
            ("h".to_string(), WordStatus::Learning),
        ]);
        let s = WordStats::of(&marks, &anki, 100, 0);
        assert_eq!(
            (
                s.known,
                s.learning,
                s.ignored,
                s.anki_known,
                s.anki_learning
            ),
            (3, 1, 1, 1, 1)
        );
        assert_eq!(s.known_by_day.len(), WORD_DAYS as usize);
        assert_eq!(s.known_by_day[0], 1, "known before the chart starts");
        assert_eq!(&s.known_by_day[28..], [2, 3]);
        assert!(s.any() && !WordStats::default().any());
    }

    // T-KNW-007, T-KNW-008: the words of the lines counted once each, the
    // score worked out from them again for other statuses
    #[test]
    fn a_count_gives_the_score_for_any_statuses() {
        let w = |e: &str, r: &str| (e.to_string(), r.to_string());
        let counted = Counted::from_lines([
            vec![w("猫", "ねこ"), w("が", "が"), w("猫", "ねこ")],
            vec![],
            vec![w("食べる", "たべる")],
        ]);
        assert_eq!(counted.words.len(), 3);
        assert_eq!(counted.lines, [vec![0, 1, 0], vec![2]]);
        assert_eq!(counted.occurrences(), [2, 1, 1]);
        let all = counted.score(|_, _| WordStatus::Unknown);
        assert_eq!(
            all,
            Score {
                words: 4,
                known: 0,
                lines: 2,
                clear: 0
            }
        );
        let some = counted.score(|e, _| match e {
            "猫" => WordStatus::Known,
            "が" => WordStatus::Ignored,
            _ => WordStatus::Learning,
        });
        assert_eq!(
            some,
            Score {
                words: 3,
                known: 2,
                lines: 2,
                clear: 2
            }
        );
        assert_eq!(
            (some.known_percent(), some.clear_percent()),
            (Some(66), Some(100))
        );
        assert_eq!(
            Counted::default()
                .score(|_, _| WordStatus::Known)
                .known_percent(),
            None
        );
    }

    // T-KNW-001: statuses kept by headword and reading, the last one set
    #[test]
    fn statuses_are_kept_by_word() {
        let dir = tempfile::tempdir().unwrap();
        let lib = Library::open(&dir.path().join("library.redb")).unwrap();
        assert!(lib.word_statuses().unwrap().is_empty(), "no table yet");
        let mark = |status, changed_at| WordMark { status, changed_at };
        lib.set_word_status("生", "せい", mark(WordStatus::Known, 1))
            .unwrap();
        lib.set_word_status("生", "なま", mark(WordStatus::Learning, 2))
            .unwrap();
        lib.set_word_status("生", "せい", mark(WordStatus::Unknown, 3))
            .unwrap();
        let all = lib.word_statuses().unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[&word_key("生", "せい")], mark(WordStatus::Unknown, 3));
        assert_eq!(all[&word_key("生", "なま")], mark(WordStatus::Learning, 2));
        assert_eq!(lib.schema(), crate::db::SCHEMA_VERSION, "no migration");
    }

    // T-KNW-005, T-KNW-006: many statuses at once; the words read from
    // Anki replaced as a whole
    #[test]
    fn lists_and_anki_words_are_kept() {
        let dir = tempfile::tempdir().unwrap();
        let lib = Library::open(&dir.path().join("library.redb")).unwrap();
        let known = WordMark {
            status: WordStatus::Known,
            changed_at: 1,
        };
        lib.set_word_statuses(&[
            ("猫".into(), String::new(), known),
            ("犬".into(), "いぬ".into(), known),
        ])
        .unwrap();
        assert_eq!(lib.word_statuses().unwrap().len(), 2);
        assert!(lib.anki_words().unwrap().is_empty());
        let first = HashMap::from([
            ("食べる".to_string(), WordStatus::Known),
            ("飲む".to_string(), WordStatus::Learning),
        ]);
        lib.set_anki_words(&first).unwrap();
        assert_eq!(lib.anki_words().unwrap(), first);
        let second = HashMap::from([("見る".to_string(), WordStatus::Learning)]);
        lib.set_anki_words(&second).unwrap();
        assert_eq!(lib.anki_words().unwrap(), second, "the old ones are gone");
    }

    // T-KNW-006: a list of words, plain or with readings and statuses, and
    // the list written read back the same
    #[test]
    fn word_lists_are_read_and_written() {
        let text = "\u{feff}expression,reading,status\n猫\n犬\tいぬ\n食べる,たべる,learning\n\n# note\n見る,みる,LEARNING\n寝る,ねる,mature\n";
        let words = parse_list(text);
        assert_eq!(
            words,
            [
                ("猫".to_string(), String::new(), WordStatus::Known),
                ("犬".to_string(), "いぬ".to_string(), WordStatus::Known),
                (
                    "食べる".to_string(),
                    "たべる".to_string(),
                    WordStatus::Learning
                ),
                ("見る".to_string(), "みる".to_string(), WordStatus::Learning),
            ]
        );
        let marks: HashMap<String, WordMark> = words
            .iter()
            .map(|(w, r, status)| {
                let mark = WordMark {
                    status: *status,
                    changed_at: 0,
                };
                (word_key(w, r), mark)
            })
            .collect();
        let written = write_list(&marks);
        assert!(written.starts_with("expression,reading,status\n"));
        let mut again = parse_list(&written);
        again.sort();
        let mut words = words;
        words.sort();
        assert_eq!(again, words);
    }

    // T-KNW-001: the newer mark wins, and a tie is settled the same way
    // everywhere
    #[test]
    fn the_newer_mark_wins() {
        let known = WordMark {
            status: WordStatus::Known,
            changed_at: 5,
        };
        let learning = WordMark {
            status: WordStatus::Learning,
            changed_at: 7,
        };
        assert!(learning.wins_over(&known));
        assert!(!known.wins_over(&learning));
        let tie = WordMark {
            changed_at: 7,
            ..known
        };
        assert!(tie.wins_over(&learning) && !learning.wins_over(&tie));
    }
}
