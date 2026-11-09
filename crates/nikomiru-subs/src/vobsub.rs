// SPDX-License-Identifier: GPL-3.0-only

//! DVD graphic subtitles (VobSub: a DVD's sub-pictures in an MPEG program
//! stream) as pictures with their times, to be read as text as PGS's are
//! (SUB-029). Nikomiru's own, written from the DVD sub-picture format:
//! Fushi reads no VobSub.
//!
//! A sub-picture is a run-length bitmap of four colour slots in two
//! interlaced fields, and the commands that show it: which of the 16
//! colours of the palette and which opacity each slot takes, where it goes,
//! and when it shows and goes. The palette is the `.idx` header's, which
//! Matroska keeps as the track's private data; without it the slots are
//! taken as a DVD usually draws them: text white, its edge black.

use crate::pgs::{Graphic, MAX_SIDE, OPEN_CUE_MS, Picture};

/// The 16 colours of a VobSub palette, as `0xRRGGBB`.
pub type Palette = [u32; 16];

/// The largest sub-picture: its size is 16 bits.
const MAX_SPU: usize = 0xffff;

/// The control sequences followed in one sub-picture, at most.
const MAX_SEQUENCES: usize = 64;

/// The palette of a VobSub `.idx` header: its `palette:` line, colours
/// left out being black; `None` without one.
pub fn palette(idx: &str) -> Option<Palette> {
    let line = idx
        .lines()
        .find_map(|l| l.trim().strip_prefix("palette:"))?;
    let mut out = [0u32; 16];
    let mut any = false;
    for (slot, hex) in out.iter_mut().zip(line.split(',')) {
        *slot = u32::from_str_radix(hex.trim(), 16).ok()? & 0x00ff_ffff;
        any = true;
    }
    any.then_some(out)
}

/// What a sub-picture's commands say.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Control {
    /// When it shows and goes, in milliseconds after its time stamp.
    start: Option<i64>,
    stop: Option<i64>,
    /// The palette's colour of each slot, and its opacity, 0 to 15.
    colors: [u8; 4],
    alpha: [u8; 4],
    /// Its place: left, top, right, bottom, the last two included.
    area: Option<(usize, usize, usize, usize)>,
    /// Where each field's pixels start.
    fields: Option<(usize, usize)>,
    /// Where the commands start: the pixels end there.
    end: usize,
}

/// A picture of the subtitles and when it shows.
#[derive(Debug, Clone)]
pub struct VobCue {
    /// Start, in milliseconds from the start of the stream.
    pub start_ms: i64,
    /// End, in milliseconds.
    pub end_ms: i64,
    spu: std::sync::Arc<[u8]>,
    control: Control,
    palette: Option<Palette>,
}

impl Graphic for VobCue {
    fn start_ms(&self) -> i64 {
        self.start_ms
    }

    fn end_ms(&self) -> i64 {
        self.end_ms
    }

    /// The picture, turned over as a PGS one is: the light of each pixel
    /// over black, its opacity taken up to the slots' highest, so the text,
    /// the lightest part, is dark on a light ground.
    fn picture(&self) -> Option<Picture> {
        let c = &self.control;
        let (left, top, right, bottom) = c.area?;
        let (width, height) = (right.checked_sub(left)? + 1, bottom.checked_sub(top)? + 1);
        if width > MAX_SIDE || height > MAX_SIDE {
            return None;
        }
        let most = u32::from(*c.alpha.iter().max()?);
        if most == 0 {
            return None;
        }
        let slots = decode(&self.spu, c, width, height)?;
        let luma = |slot: usize| -> u32 {
            let rgb = match &self.palette {
                Some(palette) => palette[usize::from(c.colors[slot] & 0x0f)],
                // as a DVD usually draws them: ground, text, edge, shade
                None => [0, 0xff_ffff, 0, 0x80_8080][slot],
            };
            let (r, g, b) = ((rgb >> 16) & 0xff, (rgb >> 8) & 0xff, rgb & 0xff);
            (299 * r + 587 * g + 114 * b) / 1000
        };
        let mut grey = vec![255u8; width * height];
        let mut alpha = vec![0u8; width * height];
        for (at, slot) in slots.iter().enumerate() {
            let slot = usize::from(*slot);
            let a = u32::from(c.alpha[slot]) * 255 / most;
            if a == 0 {
                continue;
            }
            grey[at] = 255 - (luma(slot) * a / 255).min(255) as u8;
            alpha[at] = a.min(255) as u8;
        }
        Some(Picture {
            width,
            height,
            grey,
            alpha,
        })
    }
}

/// The cues of a VobSub track copied out as an MPEG program stream, with
/// its palette, if known. A damaged sub-picture is left out; one that never
/// says when it goes lasts until the next, at most [`OPEN_CUE_MS`].
pub fn parse(stream: &[u8], palette: Option<Palette>) -> Vec<VobCue> {
    let spus: Vec<(i64, Vec<u8>)> = sub_pictures(stream);
    let mut cues: Vec<VobCue> = Vec::new();
    for (pts, spu) in spus {
        let Some(control) = control(&spu) else {
            continue;
        };
        let at = pts / 90;
        let start_ms = at + control.start.unwrap_or(0);
        if let Some(last) = cues.last_mut()
            && last.end_ms > start_ms
        {
            last.end_ms = start_ms.max(last.start_ms + 1);
        }
        // a sub-picture that only clears the screen shows nothing
        if control.start.is_none() && control.stop.is_some() {
            continue;
        }
        let end_ms = control
            .stop
            .map_or(start_ms + OPEN_CUE_MS, |stop| at + stop);
        cues.push(VobCue {
            start_ms,
            end_ms: end_ms.max(start_ms + 1),
            spu: spu.into(),
            control,
            palette,
        });
    }
    cues
}

fn u16_at(d: &[u8], o: usize) -> Option<usize> {
    Some(usize::from(*d.get(o)?) << 8 | usize::from(*d.get(o + 1)?))
}

/// A time of the commands, in units of 1024 ticks of 90 kHz, as
/// milliseconds.
fn date_ms(date: usize) -> i64 {
    (date as i64 * 1024) / 90
}

/// The commands of a sub-picture; `None` when it is not one.
fn control(spu: &[u8]) -> Option<Control> {
    let size = u16_at(spu, 0)?;
    if size > spu.len() || size < 4 {
        return None;
    }
    let spu = &spu[..size];
    let first = u16_at(spu, 2)?;
    let mut c = Control {
        end: first,
        ..Control::default()
    };
    let mut at = first;
    for _ in 0..MAX_SEQUENCES {
        let date = date_ms(u16_at(spu, at)?);
        let next = u16_at(spu, at + 2)?;
        let mut i = at + 4;
        while let Some(&command) = spu.get(i) {
            i += 1;
            match command {
                0x00 | 0x01 => {
                    c.start.get_or_insert(date);
                }
                0x02 => {
                    if c.stop.is_none() && c.start.is_none_or(|s| date > s) {
                        c.stop = Some(date);
                    }
                }
                0x03 | 0x04 => {
                    let b = spu.get(i..i + 2)?;
                    let nibbles = [b[1] & 0x0f, b[1] >> 4, b[0] & 0x0f, b[0] >> 4];
                    if command == 0x03 {
                        c.colors = nibbles;
                    } else {
                        c.alpha = nibbles;
                    }
                    i += 2;
                }
                0x05 => {
                    let b = spu.get(i..i + 6)?;
                    let high = |x: u8, y: u8| usize::from(x) << 4 | usize::from(y >> 4);
                    let low = |x: u8, y: u8| usize::from(x & 0x0f) << 8 | usize::from(y);
                    c.area = Some((
                        high(b[0], b[1]),
                        high(b[3], b[4]),
                        low(b[1], b[2]),
                        low(b[4], b[5]),
                    ));
                    i += 6;
                }
                0x06 => {
                    c.fields = Some((u16_at(spu, i)?, u16_at(spu, i + 2)?));
                    i += 4;
                }
                // a change of colours along the lines: its size, then skipped
                0x07 => i += u16_at(spu, i)?,
                _ => break,
            }
        }
        if next <= at || next >= spu.len() {
            break;
        }
        at = next;
    }
    Some(c)
}

/// Reads the pixels' run-length codes four bits at a time.
struct Nibbles<'a> {
    data: &'a [u8],
    at: usize,
    end: usize,
}

impl Nibbles<'_> {
    fn next(&mut self) -> Option<u8> {
        if self.at >= self.end {
            return None;
        }
        let byte = *self.data.get(self.at / 2)?;
        let n = if self.at.is_multiple_of(2) {
            byte >> 4
        } else {
            byte & 0x0f
        };
        self.at += 1;
        Some(n)
    }

    /// A run: its length, `None` for the rest of the line, and its slot.
    fn run(&mut self) -> Option<(Option<usize>, u8)> {
        let mut v = u32::from(self.next()?);
        for limit in [0x4, 0x10, 0x40] {
            if v >= limit {
                break;
            }
            v = v << 4 | u32::from(self.next()?);
        }
        let slot = (v & 3) as u8;
        Some(((v >= 4).then_some((v >> 2) as usize), slot))
    }
}

/// The slot of each pixel, row by row: the even rows from the first field,
/// the odd ones from the second. What the data does not reach stays the
/// ground.
fn decode(spu: &[u8], c: &Control, width: usize, height: usize) -> Option<Vec<u8>> {
    let (top, bottom) = c.fields?;
    let end = c.end.min(spu.len()) * 2;
    let mut out = vec![0u8; width * height];
    for (field, start) in [(0, top), (1, bottom)] {
        let mut nibbles = Nibbles {
            data: spu,
            at: start * 2,
            end,
        };
        'rows: for y in (field..height).step_by(2) {
            let row = &mut out[y * width..(y + 1) * width];
            let mut x = 0;
            while x < width {
                let Some((run, slot)) = nibbles.run() else {
                    break 'rows;
                };
                let to = run.map_or(width, |r| (x + r).min(width));
                row[x..to].fill(slot);
                x = to;
            }
            // each line starts on a whole byte
            nibbles.at += nibbles.at % 2;
        }
    }
    Some(out)
}

/// The sub-pictures of the first subtitle stream of an MPEG program
/// stream, each with its time stamp in 90 kHz ticks, put back together
/// from their packets.
fn sub_pictures(ps: &[u8]) -> Vec<(i64, Vec<u8>)> {
    let mut out = Vec::new();
    let mut stream: Option<u8> = None;
    let mut open: Option<(i64, usize, Vec<u8>)> = None;
    let mut i = 0;
    while i + 4 <= ps.len() {
        if ps[i..i + 3] != [0, 0, 1] {
            i += 1;
            continue;
        }
        let code = ps[i + 3];
        match code {
            // a pack: MPEG-2's has its stuffing, MPEG-1's is shorter
            0xba => {
                i += match ps.get(i + 4) {
                    Some(b) if b & 0xc0 == 0x40 => {
                        14 + usize::from(ps.get(i + 13).copied().unwrap_or(0) & 7)
                    }
                    _ => 12,
                };
            }
            0xb9 => i += 4,
            0xbb..=0xff => {
                let Some(len) = u16_at(ps, i + 4) else { break };
                let body = i + 6;
                let next = (body + len).min(ps.len());
                if code == 0xbd
                    && let Some((pts, payload)) = pes_payload(&ps[body..next])
                    && let Some((&id, data)) = payload.split_first()
                    && (0x20..0x40).contains(&id)
                    && *stream.get_or_insert(id) == id
                {
                    // a new sub-picture starts with a time stamp
                    if let Some(pts) = pts {
                        let size = u16_at(data, 0).unwrap_or(0);
                        open = Some((pts, size, data.to_vec()));
                    } else if let Some((_, _, spu)) = open.as_mut() {
                        spu.extend_from_slice(data);
                    }
                    if let Some((pts, size, spu)) = &mut open
                        && spu.len() >= *size
                    {
                        if *size >= 4 && *size <= MAX_SPU {
                            spu.truncate(*size);
                            out.push((*pts, std::mem::take(spu)));
                        }
                        open = None;
                    }
                }
                i = next;
            }
            _ => i += 4,
        }
    }
    out
}

/// A PES packet's time stamp and payload, of MPEG-2 or MPEG-1.
fn pes_payload(body: &[u8]) -> Option<(Option<i64>, &[u8])> {
    let pts = |b: &[u8]| -> Option<i64> {
        let b = b.get(..5)?;
        Some(
            (i64::from(b[0] >> 1) & 7) << 30
                | i64::from(b[1]) << 22
                | i64::from(b[2] >> 1) << 15
                | i64::from(b[3]) << 7
                | i64::from(b[4] >> 1),
        )
    };
    if body.first()? & 0xc0 == 0x80 {
        let (flags, length) = (*body.get(1)?, usize::from(*body.get(2)?));
        let header = body.get(3..3 + length)?;
        let stamp = if flags & 0x80 != 0 { pts(header) } else { None };
        return Some((stamp, &body[3 + length..]));
    }
    let mut i = 0;
    while *body.get(i)? == 0xff {
        i += 1;
    }
    if body[i] & 0xc0 == 0x40 {
        i += 2;
    }
    match body.get(i)? >> 4 {
        0x2 => Some((pts(&body[i..]), body.get(i + 5..)?)),
        0x3 => Some((pts(&body[i..]), body.get(i + 10..)?)),
        _ => Some((None, body.get(i + 1..)?)),
    }
}

/// VobSub streams written by the tests (`test-support`): Nikomiru reads
/// VobSub and never writes it.
#[cfg(any(test, feature = "test-support"))]
pub mod testing {
    use super::Palette;

    /// The palette of the tests: black ground, white text, black edge,
    /// grey shade, then black.
    pub const PALETTE: Palette = [
        0x000000, 0xffffff, 0x000000, 0x808080, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    ];

    struct Writer {
        out: Vec<u8>,
        half: bool,
    }

    impl Writer {
        fn nibble(&mut self, n: u8) {
            if self.half {
                if let Some(last) = self.out.last_mut() {
                    *last |= n & 0x0f;
                }
            } else {
                self.out.push(n << 4);
            }
            self.half = !self.half;
        }

        fn run(&mut self, len: usize, slot: u8) {
            let v = (len << 2) as u32 | u32::from(slot);
            let nibbles = match len {
                0 => 4,
                1..=3 => 1,
                4..=15 => 2,
                16..=63 => 3,
                _ => 4,
            };
            for i in (0..nibbles).rev() {
                self.nibble((v >> (4 * i)) as u8 & 0x0f);
            }
        }

        fn row(&mut self, row: &[u8]) {
            let mut x = 0;
            while x < row.len() {
                let slot = row[x];
                let mut len = 1;
                while x + len < row.len() && row[x + len] == slot && len < 255 {
                    len += 1;
                }
                if x + len == row.len() && len > 3 {
                    // the rest of the line
                    self.run(0, slot);
                } else {
                    self.run(len, slot);
                }
                x += len;
            }
            self.half = false;
        }
    }

    /// One sub-picture: `rows` of slots (0 ground, 1 text, 2 edge, 3
    /// shade) at `x, y`, shown from its time stamp for `ms` milliseconds.
    pub fn spu(x: u16, y: u16, rows: &[Vec<u8>], ms: u32) -> Vec<u8> {
        spu_with(x, y, rows, ms, [0, 15, 15, 15])
    }

    /// The same with each slot's opacity, 0 to 15.
    pub fn spu_with(x: u16, y: u16, rows: &[Vec<u8>], ms: u32, alpha: [u8; 4]) -> Vec<u8> {
        let (w, h) = (rows[0].len() as u16, rows.len() as u16);
        let mut pixels = Writer {
            out: Vec::new(),
            half: false,
        };
        for row in rows.iter().step_by(2) {
            pixels.row(row);
        }
        let bottom = 4 + pixels.out.len() as u16;
        for row in rows.iter().skip(1).step_by(2) {
            pixels.row(row);
        }
        let first = 4 + pixels.out.len() as u16;
        let (x2, y2) = (x + w - 1, y + h - 1);
        // its second sequence's place is put in when its length is known
        let mut first_seq = vec![0, 0, 0, 0];
        first_seq.extend([0x03, 0x32, 0x10, 0x04]);
        first_seq.extend([alpha[3] << 4 | alpha[2], alpha[1] << 4 | alpha[0], 0x05]);
        first_seq.extend([
            (x >> 4) as u8,
            ((x & 0x0f) << 4) as u8 | (x2 >> 8) as u8,
            x2 as u8,
            (y >> 4) as u8,
            ((y & 0x0f) << 4) as u8 | (y2 >> 8) as u8,
            y2 as u8,
        ]);
        first_seq.push(0x06);
        first_seq.extend(4u16.to_be_bytes());
        first_seq.extend(bottom.to_be_bytes());
        first_seq.extend([0x01, 0xff]);
        let second = first + first_seq.len() as u16;
        first_seq[2..4].copy_from_slice(&second.to_be_bytes());
        let date = (ms * 90 / 1024) as u16;
        let mut second_seq = date.to_be_bytes().to_vec();
        second_seq.extend(second.to_be_bytes());
        second_seq.extend([0x02, 0xff]);
        let size = first + (first_seq.len() + second_seq.len()) as u16;
        let mut out = size.to_be_bytes().to_vec();
        out.extend(first.to_be_bytes());
        out.extend(pixels.out);
        out.extend(first_seq);
        out.extend(second_seq);
        out
    }

    fn pts_bytes(pts: u64) -> [u8; 5] {
        [
            0x21 | ((pts >> 29) & 0x0e) as u8,
            (pts >> 22) as u8,
            ((pts >> 14) & 0xfe) as u8 | 1,
            (pts >> 7) as u8,
            ((pts << 1) & 0xfe) as u8 | 1,
        ]
    }

    /// Sub-pictures at their times in an MPEG-2 program stream, in packets
    /// of at most `chunk` bytes, as a `.sub` file has them; with where each
    /// starts, for its `.idx`.
    pub fn program_stream(spus: &[(u32, Vec<u8>)], chunk: usize) -> (Vec<u8>, Vec<usize>) {
        let mut out = Vec::new();
        let mut starts = Vec::new();
        for (ms, spu) in spus {
            starts.push(out.len());
            for (n, piece) in spu.chunks(chunk.max(1)).enumerate() {
                out.extend([0, 0, 1, 0xba, 0x44, 0, 4, 0, 4, 1, 0x01, 0x89, 0xc3, 0xf8]);
                let header: Vec<u8> = if n == 0 {
                    pts_bytes(u64::from(*ms) * 90).to_vec()
                } else {
                    Vec::new()
                };
                let len = 3 + header.len() + 1 + piece.len();
                out.extend([0, 0, 1, 0xbd]);
                out.extend((len as u16).to_be_bytes());
                out.extend([0x81, if n == 0 { 0x80 } else { 0 }, header.len() as u8]);
                out.extend(header);
                out.push(0x20);
                out.extend(piece);
            }
        }
        out.extend([0, 0, 1, 0xb9]);
        (out, starts)
    }

    /// The `.idx` of a `.sub` file: its size, palette and where each
    /// sub-picture is, as ffmpeg reads one.
    pub fn idx(palette: &Palette, times: &[u32], starts: &[usize]) -> String {
        let colours: Vec<String> = palette.iter().map(|c| format!("{c:06x}")).collect();
        let mut out = format!(
            "# VobSub index file, v7 (do not modify this line!)\nsize: 720x480\npalette: {}\n\nid: ja, index: 0\n",
            colours.join(", ")
        );
        for (ms, start) in times.iter().zip(starts) {
            let (h, m, s, f) = (ms / 3_600_000, ms / 60_000 % 60, ms / 1000 % 60, ms % 1000);
            out.push_str(&format!(
                "timestamp: {h:02}:{m:02}:{s:02}:{f:03}, filepos: {start:09x}\n"
            ));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::testing::{PALETTE, idx, program_stream, spu};
    use super::*;
    use crate::pgs::{lines, testing::block};

    // T-SUB-029: the palette of an `.idx` header
    #[test]
    fn the_palette_is_read_from_the_header() {
        let header = idx(&PALETTE, &[], &[]);
        assert_eq!(palette(&header), Some(PALETTE));
        assert_eq!(
            palette("size: 720x480\npalette: ffffff, 00ff00\n").map(|p| p[1]),
            Some(0x00ff00)
        );
        assert_eq!(palette("size: 720x480\n"), None);
        assert_eq!(palette("palette: zz\n"), None);
    }

    // T-SUB-029: sub-pictures out of a program stream, with their times,
    // places and pixels; one cut across packets is put back together
    #[test]
    fn sub_pictures_are_read_with_their_times() {
        let spus = [
            (1000, spu(100, 400, &block(60, 21), 1500)),
            (4000, spu(120, 380, &block(300, 40), 2000)),
        ];
        let (ps, _) = program_stream(&spus, 200);
        let cues = parse(&ps, Some(PALETTE));
        let times: Vec<_> = cues.iter().map(|c| (c.start_ms, c.end_ms)).collect();
        // the stop's unit is 1024 ticks: a few milliseconds short
        assert_eq!(times, [(1000, 2490), (4000, 5991)]);
        let picture = cues[0].picture().unwrap();
        assert_eq!((picture.width, picture.height), (60, 21));
        // the edge black, the text white: turned over, dark text on light
        assert_eq!(picture.grey[0], 255);
        assert_eq!(picture.alpha[0], 255);
        assert_eq!(picture.grey[61], 0);
        assert_eq!(picture.grey[60 * 19 + 30], 0, "an odd row");
        assert_eq!(picture.grey[60 * 20 + 30], 255, "the last row's edge");
        let lines = lines(&cues[1].picture().unwrap());
        assert_eq!(lines.len(), 1);
    }

    // T-SUB-029: without the palette, the slots as a DVD draws them
    #[test]
    fn without_a_palette_the_text_is_white() {
        let (ps, _) = program_stream(&[(0, spu(0, 0, &block(40, 20), 1000))], 2000);
        let picture = parse(&ps, None)[0].picture().unwrap();
        assert_eq!(picture.grey[41], 0);
        assert_eq!(picture.grey[0], 255);
    }

    // T-SUB-029: a sub-picture that never ends lasts until the next; a
    // damaged one, a cut stream, MPEG-1 packets
    #[test]
    fn damaged_streams_keep_what_was_read() {
        let mut open = spu(0, 0, &block(40, 20), 1000);
        // its stop command made a no-op
        let len = open.len();
        open[len - 2] = 0xfe;
        let (ps, _) = program_stream(
            &[(1000, open), (3000, spu(0, 0, &block(40, 20), 500))],
            2000,
        );
        let cues = parse(&ps, None);
        let times: Vec<_> = cues.iter().map(|c| (c.start_ms, c.end_ms)).collect();
        assert_eq!(times, [(1000, 3000), (3000, 3489)]);
        let cut = &ps[..ps.len() - 30];
        assert_eq!(parse(cut, None).len(), 1);
        assert!(parse(&[0, 0, 1, 0xbd, 0xff], None).is_empty());
        assert!(parse(&[], None).is_empty());
        let mut broken = spu(0, 0, &block(40, 20), 1000);
        broken[2] = 0xff;
        let (ps, _) = program_stream(&[(0, broken)], 2000);
        assert!(parse(&ps, None).is_empty());
        // MPEG-1 packets, as ffmpeg's `-f mpeg` writes them
        let spu = spu(0, 0, &block(40, 20), 1000);
        let mut mpeg1 = vec![0, 0, 1, 0xbd];
        mpeg1.extend(((6 + spu.len()) as u16).to_be_bytes());
        mpeg1.extend([0x21, 0x00, 0x03, 0x5f, 0x91, 0x20]);
        mpeg1.extend(&spu);
        let cues = parse(&mpeg1, None);
        assert_eq!(cues.iter().map(|c| c.start_ms).collect::<Vec<_>>(), [500]);
    }
}
