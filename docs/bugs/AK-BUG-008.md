# AK-BUG-008 — Manga columns left unread: a balloon's first column, short columns and dots

- **Requirement**: MNG-012.
- **Found**: 2026-11-19, by the maintainer on a real volume (Kaiji volume 1,
  page 8), with 2.173.0.
- **Environment**: Linux, comic-text-detector and manga-ocr as pinned, the
  page read ahead by the app.
- **Observed**: in four balloons the original text stayed under the overlay
  or was covered without being written again: the large first column 脱税
  (the detector's line map had no line for it), the short column 元に, the
  column of dots after 帰ったんだよ, and the dots of どうだ．．．！ and
  恐れいったか．．．, which were covered as furigana.
- **Causes**:
  1. A column the detector left unmarked was added only between two marked
     lines: a balloon's first or last column never was.
  2. That column was measured with a character size from the caller's count
     of characters; the app passes an estimate from the detector's lines, a
     far larger character than the whole block's reading gave, so short
     columns such as 元に were lost in the app and found by the bench.
  3. A line narrower than 60 % of the others was dropped as furigana or a
     stray stroke, wherever it was: a column of dots a column away was too.
  4. The page cache kept results under the same version after the layout
     changed.
- **Fix**: the missed columns are measured with a character as wide as the
  lines found; a first or last column is added inside the block's own box,
  a column's gap from its neighbour (a third of one for a narrow one, whose
  ink must then be in three pieces at least: dots, not one stroke); a narrow
  line is dropped only when it touches a wider one, as furigana do, and is
  kept as dots otherwise; the page cache's version goes to 12.
- **Regression**: `a_first_column_and_a_column_of_dots_are_lines` in
  `crates/nikomiru-ocr/src/lib.rs`; the real page checked in the app with
  `l7_a_real_manga_is_read_ahead` (all ten balloons whole), and the 30 pages
  of Nana volume 1 measured again (`docs/qa/2.173.1.md`).
