// SPDX-License-Identifier: GPL-3.0-only

//! The lines of an audiobook found in its book (ABK-005): each line's text
//! looked for in the book's, in order, so that the book can follow the
//! sound. Only letters and digits are compared: punctuation, spaces and
//! line breaks differ between a book and its timed text, furigana are not
//! in a paragraph's text already.

use std::collections::HashMap;

use crate::Book;

/// The characters a line must share with the book, from its start, to be
/// found there.
const PROBE: usize = 8;
/// How far past the last line found the next one is looked for first, in
/// characters: a line skipped is not a reason to jump across the book.
/// Further, only a line of [`PROBE`] characters or more is looked for.
const WINDOW: usize = 4000;

fn keep(c: char) -> bool {
    c.is_alphanumeric()
}

/// Where each line is in `book`: its chapter and paragraph, or `None` when
/// it is not found. Lines are looked for in order: a line is found after
/// the one before it, near it first, then anywhere after it if it is long
/// enough to be told apart.
pub fn align<S: AsRef<str>>(book: &Book, lines: &[S]) -> Vec<Option<(usize, usize)>> {
    // the book's characters, and the paragraph of each
    let mut text: Vec<char> = Vec::new();
    let mut place: Vec<(usize, usize)> = Vec::new();
    for (c, chapter) in book.chapters.iter().enumerate() {
        for (p, paragraph) in chapter.paragraphs.iter().enumerate() {
            for ch in paragraph.text.chars().filter(|c| keep(*c)) {
                text.push(ch);
                place.push((c, p));
            }
        }
    }
    // where each run of PROBE characters starts, for the long lines
    let mut index: HashMap<&[char], Vec<usize>> = HashMap::new();
    for (i, run) in text.windows(PROBE).enumerate() {
        index.entry(run).or_default().push(i);
    }
    let find_near = |needle: &[char], from: usize| -> Option<usize> {
        let to = (from + WINDOW + needle.len()).min(text.len());
        if from + needle.len() > to {
            return None;
        }
        (from..=to - needle.len()).find(|&i| text[i..i + needle.len()] == *needle)
    };
    let mut cursor = 0;
    lines
        .iter()
        .map(|line| {
            let line: Vec<char> = line.as_ref().chars().filter(|c| keep(*c)).collect();
            if line.is_empty() {
                return None;
            }
            let probe = &line[..line.len().min(PROBE)];
            let at = find_near(probe, cursor).or_else(|| {
                // anywhere after, only for a line long enough to be told apart
                let starts = index.get(probe)?;
                let i = starts.partition_point(|&s| s < cursor);
                starts.get(i).copied()
            })?;
            cursor = at + line.len().min(text.len() - at);
            place.get(at).copied()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Chapter, Paragraph};

    fn book(chapters: &[&[&str]]) -> Book {
        Book {
            chapters: chapters
                .iter()
                .map(|c| Chapter {
                    title: None,
                    paragraphs: c
                        .iter()
                        .map(|t| Paragraph {
                            text: (*t).to_string(),
                            ..Paragraph::default()
                        })
                        .collect(),
                })
                .collect(),
            ..Book::default()
        }
    }

    // T-ABK-005: lines found in order, across paragraphs and chapters,
    // whatever their punctuation and spaces
    #[test]
    fn lines_are_found_in_their_book() {
        let b = book(&[
            &[
                "第一章",
                "吾輩は猫である。名前はまだ無い。",
                "どこで生れたかとんと見当がつかぬ。",
            ],
            &[
                "第二章",
                "何でも薄暗いじめじめした所で泣いていた事だけは記憶している。",
            ],
        ]);
        let lines = [
            "吾輩は猫である",
            "名前は、まだ無い",
            "どこで生れたか\nとんと見当がつかぬ",
            "（拍手）",
            "",
            "何でも薄暗い　じめじめした所で",
        ];
        assert_eq!(
            align(&b, &lines),
            [
                Some((0, 1)),
                Some((0, 1)),
                Some((0, 2)),
                None,
                None,
                Some((1, 1))
            ]
        );
    }

    // T-ABK-005: a short line found again later is not taken from before
    // the line found last
    #[test]
    fn lines_go_forward() {
        let b = book(&[&["はい。", "長い長い物語の始まりです。", "はい。"]]);
        assert_eq!(
            align(&b, &["長い長い物語の始まりです", "はい"]),
            [Some((0, 1)), Some((0, 2))]
        );
        assert_eq!(align(&book(&[]), &["何か"]), [None]);
    }
}
