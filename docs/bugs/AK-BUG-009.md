# AK-BUG-009 — Manga text too small, two columns read as one

- **Requirement**: MNG-012.
- **Found**: 2026-11-19, by the maintainer in Kaiji volume 1 with 2.174.0: letters still too small, columns not read, and a wrong transcription now and then.
- **Observed** on page 18: ベンツじゃ and ないと, marked by the detector as one line twice as wide as the others, were read as one (・ンツじゃないと) and written in one narrow column; the …！ of 信じてください was covered and not written again; across the volume the text written was smaller than the original's, most of all in balloons with a column of dots.
- **Causes**:
  1. A detector mark wider than the others was split only from 1.8 times the median width; this one was 1.76.
  2. A ！ or ？ ends in a dot after its stroke: the dots test allowed a long piece only as the last one.
  3. The balloon's size was 0.8 times the size most of its lines hold, and lines of dots, narrow, counted in it: the characters came out at about 85 % of the original's width.
- **Fix**: a line far wider than its neighbours (1.4 times) is split where a clear white gap parts it, each part two fifths of its width at least and a character and a half long; the long piece of a dotted line may be among its last two; the size is the one most lines of text hold (lines of dots left out), at the line's width; the page cache's version goes to 13.
- **Regression**: `two_columns_marked_as_one_are_split` in `crates/nikomiru-ocr/src/lib.rs`; Kaiji volume 1 pages 5–24 and Nana volume 1 pages 10–39 measured again (`docs/qa/2.174.1.md`).
