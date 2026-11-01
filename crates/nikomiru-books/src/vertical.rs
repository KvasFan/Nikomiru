// SPDX-License-Identifier: GPL-3.0-only

//! Vertical writing (BOK-009): where each character of a chapter goes, in
//! columns of `rows` cells. Pure: the interface draws the cells, columns
//! counted from the right. Japanese fills square cells, so no measuring is
//! needed, and a chapter is laid out at once.

use crate::Paragraph;

/// How a character stands in its cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stance {
    /// As it is.
    Upright,
    /// Turned a quarter clockwise: long vowel marks, brackets, Latin.
    Turned,
    /// In the top right corner: `、` and `。`.
    Corner,
}

/// A character in its place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    /// Its paragraph.
    pub paragraph: usize,
    /// Its byte in the paragraph's text.
    pub offset: usize,
    /// The character.
    pub ch: char,
    /// Its column, from the first.
    pub column: usize,
    /// Its row in the column; `rows` for one hung below it.
    pub row: usize,
    /// How it stands.
    pub stance: Stance,
}

/// A chapter's cells, and how many columns they take.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Columns {
    /// The cells, in reading order.
    pub cells: Vec<Cell>,
    /// The pictures: a paragraph and the first column of its page.
    pub pictures: Vec<(usize, usize)>,
    /// The columns in all.
    pub columns: usize,
}

impl Columns {
    /// The first column of a paragraph.
    pub fn column_of(&self, paragraph: usize) -> usize {
        self.cells
            .iter()
            .find(|c| c.paragraph >= paragraph)
            .map(|c| c.column)
            .into_iter()
            .chain(
                self.pictures
                    .iter()
                    .filter(|(p, _)| *p >= paragraph)
                    .map(|(_, column)| *column),
            )
            .min()
            .unwrap_or(0)
    }

    /// The paragraph that starts or goes on at `column`.
    pub fn paragraph_at(&self, column: usize) -> usize {
        if let Some((paragraph, _)) = self.pictures.iter().find(|(_, c)| *c == column) {
            return *paragraph;
        }
        self.cells
            .iter()
            .find(|c| c.column >= column)
            .map_or(0, |c| c.paragraph)
    }
}

fn stance(c: char) -> Stance {
    match c {
        '、' | '。' | '，' | '．' => Stance::Corner,
        'ー' | '—' | '―' | '–' | '－' | '〜' | '～' | '…' | '‥' | '「' | '」' | '『' | '』'
        | '（' | '）' | '(' | ')' | '【' | '】' | '〈' | '〉' | '《' | '》' | '［' | '］' | '['
        | ']' | '〔' | '〕' | '｛' | '｝' | '＝' | '=' => Stance::Turned,
        _ if c.is_ascii_graphic() => Stance::Turned,
        _ => Stance::Upright,
    }
}

/// Whether a line may not start with it (kinsoku): it hangs below the last.
fn closes(c: char) -> bool {
    matches!(
        c,
        '、' | '。' | '，' | '．' | '」' | '』' | '）' | '】' | '〉' | '》' | '］' | '！' | '？'
    )
}

/// Lays out `paragraphs` in columns of `rows` cells, on pages of `per_page`
/// columns. Each paragraph starts a column, a heading leaves one empty after
/// it, and a picture takes a page of its own (BOK-010).
pub fn lay_out(paragraphs: &[Paragraph], rows: usize, per_page: usize) -> Columns {
    let rows = rows.max(2);
    let per_page = per_page.max(1);
    let (mut cells, mut pictures, mut column) = (Vec::new(), Vec::new(), 0usize);
    for (index, item) in paragraphs.iter().enumerate() {
        if item.image.is_some() {
            let page = column.div_ceil(per_page) * per_page;
            pictures.push((index, page));
            column = page + per_page;
            continue;
        }
        let mut row = 0;
        for (offset, ch) in item.text.char_indices() {
            if row == rows {
                if closes(ch) {
                    cells.push(Cell {
                        paragraph: index,
                        offset,
                        ch,
                        column,
                        row,
                        stance: stance(ch),
                    });
                    column += 1;
                    row = 0;
                    continue;
                }
                column += 1;
                row = 0;
            }
            cells.push(Cell {
                paragraph: index,
                offset,
                ch,
                column,
                row,
                stance: stance(ch),
            });
            row += 1;
        }
        if row > 0 {
            column += 1;
        }
        if item.heading {
            column += 1;
        }
    }
    Columns {
        cells,
        pictures,
        columns: column,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paragraph(text: &str) -> Paragraph {
        Paragraph {
            text: text.into(),
            ..Paragraph::default()
        }
    }

    fn places(columns: &Columns) -> Vec<(char, usize, usize)> {
        columns
            .cells
            .iter()
            .map(|c| (c.ch, c.column, c.row))
            .collect()
    }

    // T-BOK-009
    #[test]
    fn columns_fill_and_paragraphs_start_one() {
        let c = lay_out(&[paragraph("一二三四五"), paragraph("六")], 2, 4);
        assert_eq!(
            places(&c),
            [
                ('一', 0, 0),
                ('二', 0, 1),
                ('三', 1, 0),
                ('四', 1, 1),
                ('五', 2, 0),
                ('六', 3, 0)
            ]
        );
        assert_eq!(c.columns, 4);
        assert_eq!(c.column_of(1), 3);
        assert_eq!(c.paragraph_at(3), 1);
        assert_eq!(c.cells[2].offset, 6, "bytes of the text");
    }

    // T-BOK-009: a line never starts with a closing mark
    #[test]
    fn closing_marks_hang() {
        let c = lay_out(&[paragraph("一二。三")], 2, 4);
        assert_eq!(
            places(&c),
            [('一', 0, 0), ('二', 0, 1), ('。', 0, 2), ('三', 1, 0)]
        );
    }

    // T-BOK-009
    #[test]
    fn marks_stand_as_vertical_text_wants() {
        assert_eq!(stance('字'), Stance::Upright);
        assert_eq!(stance('あ'), Stance::Upright);
        assert_eq!(stance('。'), Stance::Corner);
        assert_eq!(stance('ー'), Stance::Turned);
        assert_eq!(stance('「'), Stance::Turned);
        assert_eq!(stance('A'), Stance::Turned);
    }

    // T-BOK-010: a picture has a page, and the text goes on after it
    #[test]
    fn a_picture_takes_a_page() {
        let picture = Paragraph {
            image: Some("a".into()),
            ..Paragraph::default()
        };
        let c = lay_out(&[paragraph("一"), picture, paragraph("二")], 9, 4);
        assert_eq!(c.pictures, [(1, 4)]);
        assert_eq!(c.column_of(1), 4);
        assert_eq!(c.column_of(2), 8);
        assert_eq!(c.paragraph_at(4), 1);
        assert_eq!(c.columns, 9);
    }

    #[test]
    fn a_heading_leaves_a_column() {
        let heading = Paragraph {
            heading: true,
            ..paragraph("題")
        };
        let c = lay_out(&[heading, paragraph("本文")], 9, 4);
        assert_eq!(c.column_of(1), 2);
    }

    #[test]
    fn nothing_lays_out_as_nothing() {
        assert_eq!(lay_out(&[], 9, 4), Columns::default());
        assert_eq!(lay_out(&[paragraph("字")], 0, 4).cells[0].row, 0);
    }
}
