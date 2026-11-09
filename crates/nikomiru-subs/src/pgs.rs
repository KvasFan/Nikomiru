// SPDX-License-Identifier: GPL-3.0-only

//! Blu-ray graphic subtitles (HDMV PGS, a `.sup` stream of segments) as
//! pictures with their times, and the pictures cut into their lines, so a
//! reading model can make text of them (SUB-028).
//!
//! The segments' parsing is translated from Fushi
//! `packages/fushi_engine/lib/media/video/bitmap_subtitle/pgs_subtitle_parser.dart`
//! and the composition from `bitmap_subtitle_cue.dart` (commit `ac8034e`).
//! Cutting a picture into lines is Nikomiru's own: Fushi finds the lines
//! with the text detector of its manga reader, which Nikomiru does not have.
//!
//! Only what reading needs: a display set's objects, palette and place; a
//! fade changes the palette and keeps the line; a picture sent again at an
//! acquisition point keeps it too. Pixels are made only when a picture is
//! asked for, since a whole film's would fill the memory.

use std::sync::Arc;

/// The largest side of an object decoded.
pub const MAX_SIDE: usize = 4096;

/// How long the last picture stays when nothing clears it.
pub const OPEN_CUE_MS: i64 = 5000;

/// A picture of the subtitles and when it shows.
#[derive(Debug, Clone)]
pub struct PgsCue {
    /// Start, in milliseconds from the start of the stream.
    pub start_ms: i64,
    /// End, in milliseconds.
    pub end_ms: i64,
    placements: Vec<Placement>,
    palette: Arc<[u32; 256]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Object {
    width: usize,
    height: usize,
    rle: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Placement {
    object: Arc<Object>,
    x: usize,
    y: usize,
    crop_x: usize,
    crop_y: usize,
    width: usize,
    height: usize,
}

/// A picture to read: dark text on a light ground, with where there is ink.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picture {
    /// Width, in pixels.
    pub width: usize,
    /// Height, in pixels.
    pub height: usize,
    /// Grey levels, row by row: 0 black, 255 white.
    pub grey: Vec<u8>,
    /// Opacity of the subtitle at each pixel, row by row.
    pub alpha: Vec<u8>,
}

/// A picture of the subtitles, of PGS or VobSub (SUB-029), and when it
/// shows.
pub trait Graphic: Sync {
    /// Start, in milliseconds from the start of the stream.
    fn start_ms(&self) -> i64;
    /// End, in milliseconds.
    fn end_ms(&self) -> i64;
    /// The picture, dark text on a light ground; `None` when nothing could
    /// be decoded.
    fn picture(&self) -> Option<Picture>;
}

impl Graphic for PgsCue {
    fn start_ms(&self) -> i64 {
        self.start_ms
    }

    fn end_ms(&self) -> i64 {
        self.end_ms
    }

    fn picture(&self) -> Option<Picture> {
        PgsCue::picture(self)
    }
}

/// The opacity from which a pixel is ink, when lines are looked for.
const INK: u8 = 96;

impl Picture {
    fn blank(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            grey: vec![255; width * height],
            alpha: vec![0; width * height],
        }
    }

    fn ink(&self, x: usize, y: usize) -> bool {
        self.alpha[y * self.width + x] >= INK
    }

    /// The part `x, y, width, height`, with `pad` blank pixels around it.
    fn crop(&self, x: usize, y: usize, width: usize, height: usize, pad: usize) -> Picture {
        let mut out = Picture::blank(width + 2 * pad, height + 2 * pad);
        for row in 0..height {
            let from = (y + row) * self.width + x;
            let to = (row + pad) * out.width + pad;
            out.grey[to..to + width].copy_from_slice(&self.grey[from..from + width]);
            out.alpha[to..to + width].copy_from_slice(&self.alpha[from..from + width]);
        }
        out
    }
}

impl PgsCue {
    /// The picture: what is on the screen, composed on black and turned
    /// over, so the text, the lightest part of a subtitle, is dark on a
    /// light ground, as the model reads best. The opacity is taken up to
    /// the palette's highest, so a frame of a fade reads as the whole line.
    /// `None` when nothing could be decoded.
    pub fn picture(&self) -> Option<Picture> {
        let left = self.placements.iter().map(|p| p.x).min()?;
        let top = self.placements.iter().map(|p| p.y).min()?;
        let right = self.placements.iter().map(|p| p.x + p.width).max()?;
        let bottom = self.placements.iter().map(|p| p.y + p.height).max()?;
        let (width, height) = (right - left, bottom - top);
        if width == 0 || height == 0 || width > MAX_SIDE * 2 || height > MAX_SIDE * 2 {
            return None;
        }
        let most = self.palette.iter().map(|c| c >> 24).max().unwrap_or(0);
        if most == 0 {
            return None;
        }
        let mut picture = Picture::blank(width, height);
        // over black: the light of each pixel, then turned over
        let mut light = vec![0u32; width * height];
        let mut alpha = vec![0u32; width * height];
        for p in &self.placements {
            let Some(indices) = decode_rle(&p.object.rle, p.object.width, p.object.height) else {
                continue;
            };
            for row in 0..p.height {
                for col in 0..p.width {
                    let index = indices[(p.crop_y + row) * p.object.width + p.crop_x + col];
                    let argb = self.palette[usize::from(index)];
                    let a = ((argb >> 24) * 255 / most).min(255);
                    if a == 0 {
                        continue;
                    }
                    let (r, g, b) = ((argb >> 16) & 0xff, (argb >> 8) & 0xff, argb & 0xff);
                    let luma = (299 * r + 587 * g + 114 * b) / 1000;
                    let at = (p.y - top + row) * width + (p.x - left + col);
                    light[at] = (luma * a + light[at] * (255 - a)) / 255;
                    alpha[at] = a + alpha[at] * (255 - a) / 255;
                }
            }
        }
        for (at, (l, a)) in light.iter().zip(&alpha).enumerate() {
            picture.grey[at] = 255 - (*l).min(255) as u8;
            picture.alpha[at] = (*a).min(255) as u8;
        }
        Some(picture)
    }
}

/// The cues of a whole `.sup` stream. A damaged or cut tail is left out;
/// what was read before it is kept.
pub fn parse(data: &[u8]) -> Vec<PgsCue> {
    let mut state = State::default();
    let mut events = Vec::new();
    let mut offset = 0;
    while offset + 13 <= data.len() {
        if &data[offset..offset + 2] != b"PG" {
            break;
        }
        let pts = u32_at(data, offset + 2);
        let kind = data[offset + 10];
        let size = u16_at(data, offset + 11);
        let body = offset + 13;
        if body + size > data.len() {
            break;
        }
        offset = body + size;
        if let Some(event) = state.apply(kind, pts, &data[body..offset]) {
            events.push(event);
        }
    }
    group(events)
}

/// A PGS RLE bitmap as palette indices; pixels the data does not reach
/// stay 0, transparent. `None` for a size that cannot be.
fn decode_rle(rle: &[u8], width: usize, height: usize) -> Option<Vec<u8>> {
    if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
        return None;
    }
    let mut out = vec![0u8; width * height];
    let (mut i, mut x, mut y) = (0, 0, 0);
    while i < rle.len() && y < height {
        let mut color = rle[i];
        i += 1;
        let mut run = 1;
        if color == 0 {
            let Some(&flag) = rle.get(i) else { break };
            i += 1;
            if flag == 0 {
                x = 0;
                y += 1;
                continue;
            }
            run = usize::from(flag & 0x3f);
            if flag & 0x40 != 0 {
                let Some(&low) = rle.get(i) else { break };
                i += 1;
                run = (run << 8) | usize::from(low);
            }
            if flag & 0x80 != 0 {
                let Some(&c) = rle.get(i) else { break };
                i += 1;
                color = c;
            }
        }
        let end = (x + run).min(width);
        if color != 0 {
            out[y * width + x..y * width + end].fill(color);
        }
        x = end;
    }
    Some(out)
}

/// BT.709 YCbCr, studio range, as ARGB.
fn argb(y: u8, cr: u8, cb: u8, alpha: u8) -> u32 {
    let y = 1.164 * (f64::from(y) - 16.0);
    let (cr, cb) = (f64::from(cr) - 128.0, f64::from(cb) - 128.0);
    let c = |v: f64| v.round().clamp(0.0, 255.0) as u32;
    let (r, g, b) = (
        c(y + 1.793 * cr),
        c(y - 0.213 * cb - 0.533 * cr),
        c(y + 2.112 * cb),
    );
    (u32::from(alpha) << 24) | (r << 16) | (g << 8) | b
}

fn u16_at(d: &[u8], o: usize) -> usize {
    usize::from(d[o]) << 8 | usize::from(d[o + 1])
}

fn u32_at(d: &[u8], o: usize) -> i64 {
    i64::from(u32::from_be_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]]))
}

/// What is on the screen when a display set ends.
struct Event {
    pts: i64,
    placements: Vec<Placement>,
    palette: Arc<[u32; 256]>,
}

struct Composed {
    object: usize,
    x: usize,
    y: usize,
    crop: Option<(usize, usize, usize, usize)>,
}

struct Pending {
    width: usize,
    height: usize,
    rle: Vec<u8>,
}

/// The segments' state: an epoch's palettes and objects; a composition
/// waits for the end of its display set.
#[derive(Default)]
struct State {
    palettes: std::collections::HashMap<u8, [u32; 256]>,
    objects: std::collections::HashMap<usize, Arc<Object>>,
    pending: std::collections::HashMap<usize, Pending>,
    pts: Option<i64>,
    palette: u8,
    composition: Option<Vec<Composed>>,
}

impl State {
    fn apply(&mut self, kind: u8, pts: i64, body: &[u8]) -> Option<Event> {
        match kind {
            0x16 => self.composition(pts, body),
            0x14 => self.palette(body),
            0x15 => self.object(body),
            0x80 => return self.flush(),
            _ => {}
        }
        None
    }

    fn composition(&mut self, pts: i64, b: &[u8]) {
        if b.len() < 11 {
            return;
        }
        if b[7] & 0x80 != 0 {
            // an epoch starts: the objects and palettes before it are gone
            self.palettes.clear();
            self.objects.clear();
            self.pending.clear();
        }
        self.pts = Some(pts);
        self.palette = b[9];
        let mut objects = Vec::new();
        let mut o = 11;
        for _ in 0..b[10] {
            if o + 8 > b.len() {
                break;
            }
            let id = u16_at(b, o);
            let cropped = b[o + 3] & 0x40 != 0;
            let (x, y) = (u16_at(b, o + 4), u16_at(b, o + 6));
            o += 8;
            let mut crop = None;
            if cropped && o + 8 <= b.len() {
                crop = Some((
                    u16_at(b, o),
                    u16_at(b, o + 2),
                    u16_at(b, o + 4),
                    u16_at(b, o + 6),
                ));
                o += 8;
            }
            objects.push(Composed {
                object: id,
                x,
                y,
                crop,
            });
        }
        self.composition = Some(objects);
    }

    fn palette(&mut self, b: &[u8]) {
        if b.len() < 2 {
            return;
        }
        let palette = self.palettes.entry(b[0]).or_insert([0; 256]);
        for entry in b[2..].as_chunks::<5>().0 {
            palette[usize::from(entry[0])] = argb(entry[1], entry[2], entry[3], entry[4]);
        }
    }

    fn object(&mut self, b: &[u8]) {
        if b.len() < 4 {
            return;
        }
        let id = u16_at(b, 0);
        let sequence = b[3];
        let mut o = 4;
        if sequence & 0x80 != 0 {
            if b.len() < 11 {
                return;
            }
            let (width, height) = (u16_at(b, o + 3), u16_at(b, o + 5));
            if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
                // a damaged object: gone with the old one of its id
                self.pending.remove(&id);
                self.objects.remove(&id);
                return;
            }
            self.pending.insert(
                id,
                Pending {
                    width,
                    height,
                    rle: Vec::new(),
                },
            );
            o += 7;
        }
        let Some(pending) = self.pending.get_mut(&id) else {
            return;
        };
        pending.rle.extend_from_slice(&b[o..]);
        if sequence & 0x40 == 0 {
            return;
        }
        if let Some(done) = self.pending.remove(&id) {
            self.objects.insert(
                id,
                Arc::new(Object {
                    width: done.width,
                    height: done.height,
                    rle: done.rle,
                }),
            );
        }
    }

    fn flush(&mut self) -> Option<Event> {
        let composition = self.composition.take()?;
        let pts = self.pts?;
        let placements = composition.iter().filter_map(|c| self.place(c)).collect();
        let palette = Arc::new(
            self.palettes
                .get(&self.palette)
                .copied()
                .unwrap_or([0; 256]),
        );
        Some(Event {
            pts,
            placements,
            palette,
        })
    }

    fn place(&self, c: &Composed) -> Option<Placement> {
        let object = self.objects.get(&c.object)?;
        let (x, y, w, h) = c.crop.unwrap_or((0, 0, object.width, object.height));
        let crop_x = x.min(object.width);
        let crop_y = y.min(object.height);
        let width = w.min(object.width - crop_x);
        let height = h.min(object.height - crop_y);
        (width > 0 && height > 0).then(|| Placement {
            object: object.clone(),
            x: c.x,
            y: c.y,
            crop_x,
            crop_y,
            width,
            height,
        })
    }
}

fn opacity(palette: &[u32; 256]) -> u32 {
    palette.iter().map(|c| c >> 24).max().unwrap_or(0)
}

/// Display sets whose objects stay the same are one cue, with the palette
/// of its most opaque frame (the first of a fade in is often clear).
fn group(events: Vec<Event>) -> Vec<PgsCue> {
    let mut cues = Vec::new();
    let mut open: Option<(i64, Vec<Placement>, Arc<[u32; 256]>)> = None;
    for event in events {
        if let Some((_, placements, palette)) = open.as_mut()
            && *placements == event.placements
        {
            if opacity(&event.palette) > opacity(palette) {
                *palette = event.palette;
            }
            continue;
        }
        if let Some((start, placements, palette)) = open.take() {
            cues.push(cue(start, Some(event.pts), placements, palette));
        }
        if !event.placements.is_empty() {
            open = Some((event.pts, event.placements, event.palette));
        }
    }
    if let Some((start, placements, palette)) = open {
        cues.push(cue(start, None, placements, palette));
    }
    cues
}

fn cue(
    start: i64,
    end: Option<i64>,
    placements: Vec<Placement>,
    palette: Arc<[u32; 256]>,
) -> PgsCue {
    let start_ms = start / 90;
    let end_ms = end.map_or(start_ms + OPEN_CUE_MS, |e| e / 90);
    PgsCue {
        start_ms,
        end_ms: end_ms.max(start_ms + 1),
        placements,
        palette,
    }
}

/// A band of rows with ink, and how far the ink goes across it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Band {
    top: usize,
    bottom: usize,
    left: usize,
    right: usize,
}

impl Band {
    fn height(&self) -> usize {
        self.bottom - self.top
    }
}

/// Rows with no ink at most this tall stay inside a line.
const ROW_GAP: usize = 2;

/// A line is read no wider than this many times its height: the model sees
/// a square, in which a longer line's characters are too thin to tell.
pub const MAX_ASPECT: usize = 8;

/// The lines of `picture`, top to bottom, each in pieces with a blank
/// margin, to be read one by one and put back together. The bands of rows
/// with ink are the lines; a band of small text right above a line and
/// within its width, its furigana, is left out; parts of a line far apart
/// (two speakers side by side) are read apart, left to right; a line wider
/// than [`MAX_ASPECT`] times its height is cut into pieces between its
/// characters.
pub fn lines(picture: &Picture) -> Vec<Vec<Picture>> {
    let mut bands: Vec<Band> = Vec::new();
    for y in 0..picture.height {
        let mut row = (0..picture.width).filter(|&x| picture.ink(x, y));
        let Some(first) = row.next() else {
            continue;
        };
        let last = row.next_back().unwrap_or(first);
        match bands.last_mut() {
            Some(band) if y - band.bottom <= ROW_GAP => {
                band.bottom = y + 1;
                band.left = band.left.min(first);
                band.right = band.right.max(last + 1);
            }
            _ => bands.push(Band {
                top: y,
                bottom: y + 1,
                left: first,
                right: last + 1,
            }),
        }
    }
    let furigana = |i: usize| {
        let (above, line) = (bands[i], bands.get(i + 1)?);
        let near = line.top - above.bottom <= line.height() / 2;
        let small = above.height() * 8 <= line.height() * 5;
        let slack = line.height() / 2;
        let within = above.left + slack >= line.left && above.right <= line.right + slack;
        Some(near && small && within)
    };
    let mut out = Vec::new();
    for (i, band) in bands.iter().enumerate() {
        if furigana(i) == Some(true) || band.height() < 4 {
            continue;
        }
        let pad = (band.height() / 6).max(4);
        for (left, right) in parts(picture, band) {
            let pieces = pieces(picture, band, left, right)
                .into_iter()
                .map(|(l, r)| picture.crop(l, band.top, r - l, band.height(), pad))
                .collect();
            out.push(pieces);
        }
    }
    out
}

/// A part of a line `left..right` in pieces at most [`MAX_ASPECT`] times
/// as wide as the band is high, cut where no ink is: of the gaps near the
/// place each piece should end, the widest, so a cut falls between
/// characters rather than inside one, such as い. A line with no gap stays
/// whole.
fn pieces(picture: &Picture, band: &Band, left: usize, right: usize) -> Vec<(usize, usize)> {
    let h = band.height();
    let most = MAX_ASPECT * h;
    if right - left <= most {
        return vec![(left, right)];
    }
    let blank = |x: usize| !(band.top..band.bottom).any(|y| picture.ink(x, y));
    // the gaps: runs of blank columns, as (start, end)
    let mut gaps: Vec<(usize, usize)> = Vec::new();
    for x in left..right {
        if !blank(x) {
            continue;
        }
        match gaps.last_mut() {
            Some(gap) if gap.1 == x => gap.1 = x + 1,
            _ => gaps.push((x, x + 1)),
        }
    }
    let count = (right - left).div_ceil(most);
    let width = (right - left).div_ceil(count);
    let mut out = Vec::new();
    let mut start = left;
    while right - start > most {
        let ideal = start + width;
        // within a character's width of where it should be, and leaving
        // the piece at least half as wide
        let cut = gaps
            .iter()
            .filter(|g| g.0 >= start + width / 2 && g.1 <= start + most)
            .filter(|g| (g.0 + g.1) / 2 + h >= ideal && (g.0 + g.1) / 2 <= ideal + h)
            .max_by_key(|g| (g.1 - g.0, std::cmp::Reverse(g.0.abs_diff(ideal))))
            .or_else(|| {
                gaps.iter()
                    .filter(|g| g.0 >= start + width / 2 && g.1 <= start + most)
                    .max_by_key(|g| g.1 - g.0)
            });
        let Some(&(gap_start, gap_end)) = cut else {
            break;
        };
        out.push((start, gap_start));
        start = gap_end;
    }
    out.push((start, right));
    out
}

/// The parts of a band, apart where no ink is for more than twice its
/// height.
fn parts(picture: &Picture, band: &Band) -> Vec<(usize, usize)> {
    let gap = band.height() * 2;
    let mut parts: Vec<(usize, usize)> = Vec::new();
    for x in band.left..band.right {
        if !(band.top..band.bottom).any(|y| picture.ink(x, y)) {
            continue;
        }
        match parts.last_mut() {
            Some(part) if x - part.1 <= gap => part.1 = x + 1,
            _ => parts.push((x, x + 1)),
        }
    }
    parts.retain(|(l, r)| r - l >= 4);
    parts
}

/// PGS streams written by the tests (`test-support`): Nikomiru reads PGS
/// and never writes it.
#[cfg(any(test, feature = "test-support"))]
pub mod testing {
    /// Encodes one row of palette indices in PGS RLE.
    fn rle_row(row: &[u8], out: &mut Vec<u8>) {
        let mut i = 0;
        while i < row.len() {
            let color = row[i];
            let mut run = 1;
            while i + run < row.len() && row[i + run] == color && run < 16_383 {
                run += 1;
            }
            match (color, run) {
                (c, 1) if c != 0 => out.push(c),
                (0, r) if r < 64 => out.extend([0, r as u8]),
                (0, r) => out.extend([0, 0x40 | (r >> 8) as u8, r as u8]),
                (c, r) if r < 64 => out.extend([0, 0x80 | r as u8, c]),
                (c, r) => out.extend([0, 0xc0 | (r >> 8) as u8, r as u8, c]),
            }
            i += run;
        }
        out.extend([0, 0]);
    }

    fn segment(out: &mut Vec<u8>, pts: u32, kind: u8, body: &[u8]) {
        out.extend(b"PG");
        out.extend(pts.to_be_bytes());
        out.extend(0u32.to_be_bytes());
        out.push(kind);
        out.extend((body.len() as u16).to_be_bytes());
        out.extend(body);
    }

    /// One display set: the objects (palette indices, row by row) at their
    /// places, white text (1) with a black outline (2), or a clear screen
    /// when `objects` is empty.
    pub fn display_set(
        out: &mut Vec<u8>,
        ms: u32,
        objects: &[(u16, u16, Vec<Vec<u8>>)],
        alpha: u8,
    ) {
        let palette = [[1, 235, 128, 128, alpha], [2, 16, 128, 128, alpha]];
        display_set_with(out, ms, objects, &palette);
    }

    /// The same with the palette's entries: index, Y, Cr, Cb, alpha. The
    /// object data is cut in two segments at least, as long ones are.
    pub fn display_set_with(
        out: &mut Vec<u8>,
        ms: u32,
        objects: &[(u16, u16, Vec<Vec<u8>>)],
        palette: &[[u8; 5]],
    ) {
        let pts = ms * 90;
        let mut pcs = vec![7, 128, 4, 56, 0x10, 0, 0, 0x80, 0, 0, objects.len() as u8];
        for (n, (x, y, _)) in objects.iter().enumerate() {
            pcs.extend((n as u16).to_be_bytes());
            pcs.extend([0, 0]);
            pcs.extend(x.to_be_bytes());
            pcs.extend(y.to_be_bytes());
        }
        segment(out, pts, 0x16, &pcs);
        let mut pds = vec![0, 0];
        pds.extend(palette.iter().flatten());
        segment(out, pts, 0x14, &pds);
        for (n, (_, _, rows)) in objects.iter().enumerate() {
            let mut rle = Vec::new();
            for row in rows {
                rle_row(row, &mut rle);
            }
            let (w, h) = (rows[0].len() as u16, rows.len() as u16);
            let size = (rle.len() / 2).clamp(1, 60_000);
            let pieces: Vec<&[u8]> = rle.chunks(size).collect();
            for (i, piece) in pieces.iter().enumerate() {
                let mut body = (n as u16).to_be_bytes().to_vec();
                let flags =
                    if i == 0 { 0x80 } else { 0 } | if i + 1 == pieces.len() { 0x40 } else { 0 };
                body.extend([0, flags]);
                if i == 0 {
                    body.extend(((rle.len() + 4) as u32).to_be_bytes()[1..].iter());
                    body.extend(w.to_be_bytes());
                    body.extend(h.to_be_bytes());
                }
                body.extend(*piece);
                segment(out, pts, 0x15, &body);
            }
        }
        segment(out, pts, 0x80, &[]);
    }

    /// A block of `w`×`h` white ink with a black edge one pixel wide.
    pub fn block(w: usize, h: usize) -> Vec<Vec<u8>> {
        (0..h)
            .map(|y| {
                (0..w)
                    .map(|x| {
                        if x == 0 || y == 0 || x == w - 1 || y == h - 1 {
                            2
                        } else {
                            1
                        }
                    })
                    .collect()
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::testing::{block, display_set};
    use super::*;

    /// Rows of `blocks`: (left, width) of each block of ink, `h` tall, in a
    /// picture `w` wide.
    fn rows(w: usize, h: usize, blocks: &[(usize, usize)]) -> Vec<Vec<u8>> {
        (0..h)
            .map(|_| {
                let mut row = vec![0; w];
                for (left, width) in blocks {
                    row[*left..left + width].fill(1);
                }
                row
            })
            .collect()
    }

    // T-SUB-028: segments, times, a clear screen, a fade, a cut tail
    #[test]
    fn display_sets_are_cues() {
        let mut sup = Vec::new();
        display_set(&mut sup, 1000, &[(100, 900, block(40, 20))], 0x40);
        // the fade: the same object, more opaque
        display_set(&mut sup, 1100, &[(100, 900, block(40, 20))], 0xff);
        display_set(&mut sup, 2500, &[], 0xff);
        display_set(
            &mut sup,
            3000,
            &[(50, 800, block(30, 10)), (50, 900, block(60, 20))],
            0xff,
        );
        display_set(&mut sup, 4000, &[(50, 900, block(60, 20))], 0xff);
        sup.extend(b"PG\0\0");
        let cues = parse(&sup);
        let times: Vec<_> = cues.iter().map(|c| (c.start_ms, c.end_ms)).collect();
        assert_eq!(
            times,
            [(1000, 2500), (3000, 4000), (4000, 4000 + OPEN_CUE_MS)]
        );
        let picture = cues[0].picture().unwrap();
        assert_eq!((picture.width, picture.height), (40, 20));
        // turned over: the white text is black, its black edge white
        assert_eq!(picture.grey[10 * 40 + 20], 0);
        assert_eq!(picture.grey[0], 255);
        assert_eq!(
            picture.alpha[10 * 40 + 20],
            255,
            "the fade's most opaque frame"
        );
        let two = cues[1].picture().unwrap();
        assert_eq!((two.width, two.height), (60, 120));
        assert_eq!(two.alpha[50 * 60 + 5], 0, "between the objects");
    }

    // T-SUB-028: damaged data does not panic
    #[test]
    fn damaged_streams_are_read_as_far_as_they_go() {
        assert!(parse(b"").is_empty());
        assert!(parse(b"PG\0\0\0\0\0\0\0\0\x16\xff\xff").is_empty());
        assert!(decode_rle(&[0, 0x40], 4, 1).is_some());
        assert!(decode_rle(&[], 0, 1).is_none());
        assert!(decode_rle(&[], MAX_SIDE + 1, 1).is_none());
        let mut sup = Vec::new();
        display_set(&mut sup, 0, &[(0, 0, block(8, 8))], 0xff);
        for cut in 0..sup.len() {
            let _ = parse(&sup[..cut]).iter().map(PgsCue::picture).count();
        }
    }

    fn picture(rows: &[Vec<u8>]) -> Picture {
        let (h, w) = (rows.len(), rows[0].len());
        let mut p = Picture::blank(w, h);
        for (y, row) in rows.iter().enumerate() {
            for (x, v) in row.iter().enumerate() {
                if *v != 0 {
                    p.alpha[y * w + x] = 255;
                    p.grey[y * w + x] = 0;
                }
            }
        }
        p
    }

    fn sizes(lines: &[Picture]) -> Vec<(usize, usize)> {
        lines.iter().map(|l| (l.width, l.height)).collect()
    }

    // T-SUB-028: a long line in pieces, cut at its widest gaps near where
    // each piece should end; a line with no gap stays whole
    #[test]
    fn a_long_line_is_read_in_pieces() {
        // 12 characters 30 wide, 6 apart, one with a narrow gap inside
        let mut blocks: Vec<(usize, usize)> = (0..12).map(|i| (i * 36, 30)).collect();
        blocks[5] = (180, 13);
        blocks.push((195, 15));
        let line = picture(&rows(440, 30, &blocks));
        let found = lines(&line);
        assert_eq!(found.len(), 1);
        let widths: Vec<usize> = found[0].iter().map(|p| p.width - 2 * 5).collect();
        assert_eq!(widths.len(), 2, "{widths:?}");
        assert!(widths.iter().all(|w| *w <= MAX_ASPECT * 30), "{widths:?}");
        assert_eq!(
            widths.iter().sum::<usize>() + 6,
            426,
            "a gap of 6 between them"
        );
        let whole = picture(&rows(400, 20, &[(0, 400)]));
        assert_eq!(lines(&whole)[0].len(), 1);
    }

    // T-SUB-028: two lines; the furigana above a line left out; a gap of a
    // stroke kept inside its line; two speakers side by side read apart
    #[test]
    fn a_picture_is_cut_into_its_lines() {
        let mut all = Vec::new();
        all.extend(rows(400, 10, &[(40, 20), (90, 20)])); // furigana
        all.extend(rows(400, 3, &[]));
        all.extend(rows(400, 14, &[(20, 200)]));
        all.extend(rows(400, 2, &[])); // a stroke's gap
        all.extend(rows(400, 16, &[(20, 200)]));
        all.extend(rows(400, 12, &[]));
        all.extend(rows(400, 30, &[(10, 100), (300, 90)]));
        let found: Vec<Picture> = lines(&picture(&all)).into_iter().flatten().collect();
        let pad = |h: usize| (h / 6).max(4) * 2;
        assert_eq!(
            sizes(&found),
            [
                (200 + pad(32), 32 + pad(32)),
                (100 + pad(30), 30 + pad(30)),
                (90 + pad(30), 30 + pad(30)),
            ]
        );
        // dark ink inside a white margin
        let first = &found[0];
        assert_eq!(first.grey[0], 255);
        let m = pad(32) / 2;
        assert_eq!(first.grey[(m + 1) * first.width + m + 1], 0);
        // a line alone, smaller text with nothing below: kept
        assert_eq!(lines(&picture(&rows(100, 10, &[(10, 50)]))).len(), 1);
        // specks are not lines
        assert!(lines(&picture(&rows(100, 2, &[(10, 50)]))).is_empty());
    }
}
