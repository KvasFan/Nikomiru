# AK-BUG-010 — A column written above its text, 10 in two cells

- **Requirement**: MNG-012.
- **Found**: 2026-11-21, when four sets of pages (Kaiji volumes 1 and 7, Nana volumes 1 and 11) were compared page by page, as the maintainer asked ("migliora ocr dei manga e visualizzazione del manga confrontando piu panelli di kaiji e nana").
- **Observed** on Kaiji volume 7 page 44: in a balloon below another, これで and 都合10人 were written about two characters above where the picture has them, the original left uncovered under the text; 10, written across in one cell in the picture (縦中横), took two cells, one under the other, and ran past the column's end.
- **Causes**:
  1. A line's extent along it was the first and last ink within about a character of the detector's mark; the balloon's outline, crossing the column just above its first character, counted as its ink.
  2. Every character had its own cell, digits too.
- **Fix**: past the mark's ends, the line stops before a stroke whose ink goes on beyond the line on both sides (an outline, a frame); in a column, two digits alone together share a cell, written side by side at half width; the page cache's version goes to 14.
- **Regression**: `an_outline_across_a_column_ends_it` in `crates/nikomiru-ocr/src/lib.rs`, `two_digits_share_a_cell_of_a_column` in `crates/nikomiru-ui/src/view/books_page.rs`; the four sets measured again (`docs/qa/2.180.0.md`).
