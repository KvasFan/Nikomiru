// SPDX-License-Identifier: GPL-3.0-only

//! Japanese text read off a picture of it (MNG-003, D-30): manga-ocr's model,
//! a vision encoder and a text decoder, run here with no native library and
//! nothing sent anywhere. The model does not ship with Nikomiru: its files
//! are fetched on request (MNG-004), pinned by revision and checksum.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::io::Cursor;
use std::path::Path;
use std::sync::Arc;

use image::{DynamicImage, GenericImageView, ImageReader, imageops::FilterType};
use tract_onnx::prelude::*;

/// The side of the square the model sees.
const SIDE: usize = 224;
/// The size of what the encoder gives: patches and their features.
const HIDDEN: (usize, usize) = (197, 768);
/// The tokens that start and end a text.
const START: i64 = 2;
const END: i64 = 3;
/// The tokens before the vocabulary's characters.
const SPECIAL: usize = 15;
/// The longest text: a balloon is far shorter, and each token takes a while.
const MAX_TOKENS: usize = 100;
/// The longest side of a picture decoded.
const MAX_SIDE: u32 = 16_384;
/// The memory a picture may take to decode.
const MAX_ALLOC: u64 = 512 << 20;
/// The smallest selection worth reading, in pixels.
const MIN_SIDE: u32 = 4;

/// A file of the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelFile<'a> {
    /// Its name on disk.
    pub name: &'a str,
    /// Where it comes from.
    pub url: &'a str,
    /// Its SHA-256, in hex.
    pub sha256: &'a str,
    /// Its size.
    pub bytes: u64,
}

/// The model: `kha-white/manga-ocr-base` (Apache-2.0, trained on Manga109-s),
/// its int8 ONNX export, at fixed revisions.
pub const MODEL: [ModelFile<'static>; 3] = [
    ModelFile {
        name: "encoder.onnx",
        url: "https://huggingface.co/onnx-community/manga-ocr-base-ONNX/resolve/f9023406bb2f6b17df67bc4a327c56ecd20611f0/onnx/encoder_model_int8.onnx",
        sha256: "ddd1af56963093795705fa38da6ce7e6567d1658e7c7359db7e13fcd37dbf279",
        bytes: 86_967_767,
    },
    ModelFile {
        name: "decoder.onnx",
        url: "https://huggingface.co/onnx-community/manga-ocr-base-ONNX/resolve/f9023406bb2f6b17df67bc4a327c56ecd20611f0/onnx/decoder_model_int8.onnx",
        sha256: "2e7177d2b0a59f1c612b694ed70c13971bee765cc2b2bc7bc9376e4753652f27",
        bytes: 29_627_936,
    },
    ModelFile {
        name: "vocab.txt",
        url: "https://huggingface.co/kha-white/manga-ocr-base/resolve/aa6573bd10b0d446cbf622e29c3e084914df9741/vocab.txt",
        sha256: "344fbb6b8bf18c57839e924e2c9365434697e0227fac00b88bb4899b78aa594d",
        bytes: 24_072,
    },
];

/// What finds the text on a whole page, so that pages are read ahead
/// (MNG-011): dmMaze's comic-text-detector (GPL-3.0, trained on Manga109-s),
/// its ONNX export, at a fixed revision.
pub const DETECTOR: ModelFile<'static> = ModelFile {
    name: "detector.onnx",
    url: "https://huggingface.co/mayocream/comic-text-detector-onnx/resolve/a5d67ec772adef819ef5b0e7aa701fcf4c8bf74a/comic-text-detector.onnx",
    sha256: "1a86ace74961413cbd650002e7bb4dcec4980ffa21b2f19b86933372071d718f",
    bytes: 94_669_756,
};

/// Every file fetched: the reader, then the detector.
pub const FILES: [ModelFile<'static>; 4] = [MODEL[0], MODEL[1], MODEL[2], DETECTOR];

/// The size of all the files fetched.
pub const fn model_bytes() -> u64 {
    MODEL[0].bytes + MODEL[1].bytes + MODEL[2].bytes + DETECTOR.bytes
}

/// Whether the model is in `dir`, as far as a look can tell: a file is put
/// there only after its checksum is checked.
pub fn installed(dir: &Path) -> bool {
    MODEL
        .iter()
        .all(|file| std::fs::metadata(dir.join(file.name)).is_ok_and(|m| m.len() == file.bytes))
}

/// Whether the detector is in `dir` too (MNG-011).
pub fn detector_installed(dir: &Path) -> bool {
    std::fs::metadata(dir.join(DETECTOR.name)).is_ok_and(|m| m.len() == DETECTOR.bytes)
}

/// Why a picture cannot be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OcrError {
    /// The model is damaged, or cannot run.
    #[error("the model: {0}")]
    Model(String),
    /// The picture cannot be decoded.
    #[error("the picture: {0}")]
    Picture(String),
    /// The selection is too small.
    #[error("the selection is too small")]
    TooSmall,
}

impl nikomiru_core::Localized for OcrError {
    fn message_id(&self) -> &'static str {
        match self {
            Self::Model(_) => "error-ocr-model",
            Self::Picture(_) => "error-ocr-picture",
            Self::TooSmall => "error-ocr-small",
        }
    }

    fn message_args(&self) -> Vec<(&'static str, String)> {
        match self {
            Self::Model(reason) | Self::Picture(reason) => vec![("reason", reason.clone())],
            Self::TooSmall => Vec::new(),
        }
    }
}

fn model_err(e: impl std::fmt::Display) -> OcrError {
    OcrError::Model(format!("{e:#}"))
}

/// The part of a page in `area`: left, top, width and height as shares of the
/// page, so the reading does not depend on how large it was drawn.
pub fn crop(page: &[u8], area: [f32; 4]) -> Result<DynamicImage, OcrError> {
    let decoded = decode_page(page)?;
    let (width, height) = decoded.dimensions();
    let (x, y, w, h) = pixels(area, width, height).ok_or(OcrError::TooSmall)?;
    Ok(decoded.crop_imm(x, y, w, h))
}

/// Crop original integer coordinates exactly, without normalised-float rounding (MNG-008).
pub fn crop_pixels(page: &[u8], bounds: [u32; 4]) -> Result<DynamicImage, OcrError> {
    let decoded = decode_page(page)?;
    let [l, t, r, b] = bounds;
    let w = r.checked_sub(l).ok_or(OcrError::TooSmall)?;
    let h = b.checked_sub(t).ok_or(OcrError::TooSmall)?;
    if w < MIN_SIDE || h < MIN_SIDE || r > decoded.width() || b > decoded.height() {
        return Err(OcrError::TooSmall);
    }
    Ok(decoded.crop_imm(l, t, w, h))
}

/// A page decoded, within the limits a page is held to.
pub fn decode(page: &[u8]) -> Result<DynamicImage, OcrError> {
    decode_page(page)
}

fn decode_page(page: &[u8]) -> Result<DynamicImage, OcrError> {
    let picture = |e: &dyn std::fmt::Display| OcrError::Picture(e.to_string());
    let mut reader = ImageReader::new(Cursor::new(page))
        .with_guessed_format()
        .map_err(|e| picture(&e))?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(MAX_ALLOC);
    reader.limits(limits);
    reader.decode().map_err(|e| picture(&e))
}

/// `area` in the pixels of a page, inside it; `None` if too little is left.
fn pixels(area: [f32; 4], width: u32, height: u32) -> Option<(u32, u32, u32, u32)> {
    let [left, top, w, h] = area.map(|v| {
        if v.is_finite() {
            v.clamp(0.0, 1.0)
        } else {
            0.0
        }
    });
    let x = (left * width as f32) as u32;
    let y = (top * height as f32) as u32;
    let right = (((left + w).min(1.0) * width as f32).ceil() as u32).min(width);
    let bottom = (((top + h).min(1.0) * height as f32).ceil() as u32).min(height);
    let (w, h) = (right.saturating_sub(x), bottom.saturating_sub(y));
    (w >= MIN_SIDE && h >= MIN_SIDE).then_some((x, y, w, h))
}

/// A grey picture, `width` by `height`, row by row: the lines of a graphic
/// subtitle (SUB-028). `None` when the sizes do not match.
pub fn grey(width: usize, height: usize, pixels: Vec<u8>) -> Option<DynamicImage> {
    let (w, h) = (u32::try_from(width).ok()?, u32::try_from(height).ok()?);
    image::GrayImage::from_raw(w, h, pixels).map(DynamicImage::ImageLuma8)
}

/// A picture as JPEG, for a card (MNG-006).
pub fn jpeg(picture: &DynamicImage) -> Result<Vec<u8>, OcrError> {
    let mut bytes = Vec::new();
    picture
        .to_rgb8()
        .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Jpeg)
        .map_err(|e| OcrError::Picture(e.to_string()))?;
    Ok(bytes)
}

/// What the model sees: grey, in a square whatever the shape, in −1…1.
fn pixel_values(picture: &DynamicImage) -> Tensor {
    let side = SIDE as u32;
    let grey = picture.grayscale().to_luma8();
    let small = image::imageops::resize(&grey, side, side, FilterType::Triangle);
    tract_ndarray::Array4::from_shape_fn((1, 3, SIDE, SIDE), |(_, _, y, x)| {
        f32::from(small[(x as u32, y as u32)].0[0]) / 127.5 - 1.0
    })
    .into()
}

/// The text of the tokens: no specials, no spaces, which the model only
/// makes up.
#[cfg(test)]
fn clean(tokens: &[i64], vocabulary: &[String]) -> String {
    let sure = vec![1.0; tokens.len()];
    clean_scored(tokens, &sure, vocabulary).text
}

/// A text read, with how sure the model was of each of its characters
/// (the probability of the token it came from).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Reading {
    /// The text.
    pub text: String,
    /// One probability per character of `text`.
    pub sure: Vec<f32>,
}

/// [`clean`] keeping each character's probability through it.
fn clean_scored(tokens: &[i64], sure: &[f32], vocabulary: &[String]) -> Reading {
    let mut raw: Vec<(char, f32)> = Vec::new();
    for (token, p) in tokens.iter().zip(sure) {
        let Some(piece) = usize::try_from(*token)
            .ok()
            .filter(|t| *t >= SPECIAL)
            .and_then(|t| vocabulary.get(t))
        else {
            continue;
        };
        for c in piece.chars().filter(|c| !c.is_whitespace()) {
            // an ellipsis as three dots
            if c == '…' {
                raw.extend([('.', *p); 3]);
            } else {
                raw.push((c, *p));
            }
        }
    }
    let (chars, sure): (Vec<char>, Vec<f32>) = full_width(&raw).into_iter().unzip();
    Reading {
        text: chars.into_iter().collect(),
        sure,
    }
}

/// What manga-ocr does last to a text: runs of dots or middle dots as dots,
/// then ASCII and digits in their full-width forms, as Japanese text writes
/// them (`!` → `！`); an ellipsis is three dots already.
fn full_width(chars: &[(char, f32)]) -> Vec<(char, f32)> {
    let dot = |c: char| c == '.' || c == '・';
    let mut out = Vec::with_capacity(chars.len());
    let mut i = 0;
    while i < chars.len() {
        let run = chars[i..].iter().take_while(|(c, _)| dot(*c)).count();
        let n = run.max(1);
        for &(c, p) in &chars[i..i + n] {
            let c = if run >= 2 { '.' } else { c };
            out.push((
                match c {
                    '!'..='~' => char::from_u32(c as u32 - 0x21 + 0xff01).unwrap_or(c),
                    _ => c,
                },
                p,
            ));
        }
        i += n;
    }
    out
}

/// The side of the square the detector sees.
const DETECT_SIDE: usize = 1024;
/// How sure the detector must be of a block.
const DETECT_CONFIDENCE: f32 = 0.4;
/// How much two blocks may overlap before the less sure one goes.
const DETECT_OVERLAP: f32 = 0.35;
/// The smallest block kept, in pixels of the page.
const DETECT_MIN: u32 = 8;

/// A block of text found on a page.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Block {
    /// Left, top, right and bottom, in pixels of the page.
    pub bounds: [u32; 4],
    /// Written top to bottom, in columns.
    pub vertical: bool,
    /// Its lines as the detector marks them, thinner than the text, in
    /// reading order: columns from the right, or rows from the top. Empty
    /// when it marked none.
    pub lines: Vec<[u32; 4]>,
}

/// The detector, loaded.
#[derive(Debug)]
pub struct Detector {
    plan: Arc<TypedSimplePlan>,
    threads: tract_linalg::multithread::Executor,
}

impl Detector {
    /// Loads the detector in `dir`.
    pub fn load(dir: &Path) -> Result<Self, OcrError> {
        let mut model = tract_onnx::onnx()
            .with_ignore_output_shapes(true)
            .model_for_path(dir.join(DETECTOR.name))
            .and_then(|m| m.with_input_fact(0, f32::fact([1, 3, DETECT_SIDE, DETECT_SIDE]).into()))
            .map_err(model_err)?;
        // the blocks and the lines of text: the mask of the text's pixels is
        // not needed, and its part of the network is left out
        let outputs = model.output_outlets().map_err(model_err)?.to_vec();
        let (Some(blocks), Some(lines)) = (outputs.first(), outputs.get(2)) else {
            return Err(OcrError::Model("the detector lacks an output".into()));
        };
        model
            .select_output_outlets(&[*blocks, *lines])
            .map_err(model_err)?;
        let plan = model
            .into_optimized()
            .and_then(|m| m.into_runnable())
            .map_err(model_err)?;
        Ok(Self {
            plan,
            threads: half_the_cores(),
        })
    }

    /// The blocks of text on `page`, in the order a manga is read: from the
    /// right, then from the top.
    pub fn find(&self, page: &DynamicImage) -> Result<Vec<Block>, OcrError> {
        let (width, height) = page.dimensions();
        let side = DETECT_SIDE as u32;
        let small = image::imageops::resize(&page.to_rgb8(), side, side, FilterType::Triangle);
        let input: Tensor = tract_ndarray::Array4::from_shape_fn(
            (1, 3, DETECT_SIDE, DETECT_SIDE),
            |(_, c, y, x)| f32::from(small[(x as u32, y as u32)].0[c]) / 255.0,
        )
        .into();
        let output =
            tract_linalg::multithread::multithread_tract_scope(self.threads.clone(), || {
                self.plan.run(tvec!(input.into()))
            })
            .map_err(model_err)?;
        let found = output
            .first()
            .ok_or_else(|| OcrError::Model("the detector gave nothing".into()))?
            .to_plain_array_view::<f32>()
            .map_err(model_err)?
            .into_dimensionality::<tract_ndarray::Ix3>()
            .map_err(model_err)?
            .index_axis(tract_ndarray::Axis(0), 0)
            .rows()
            .into_iter()
            .filter(|row| row.len() >= 5)
            .map(|row| ([row[0], row[1], row[2], row[3]], row[4]))
            .collect::<Vec<_>>();
        let scale = (width as f32 / side as f32, height as f32 / side as f32);
        let mut kept = blocks(found, scale, (width, height));
        let map = output
            .get(1)
            .ok_or_else(|| OcrError::Model("the detector gave no lines".into()))?
            .to_plain_array_view::<f32>()
            .map_err(model_err)?
            .into_dimensionality::<tract_ndarray::Ix4>()
            .map_err(model_err)?;
        let map = map
            .index_axis(tract_ndarray::Axis(0), 0)
            .index_axis_move(tract_ndarray::Axis(0), 0);
        let lines: Vec<[u32; 4]> = components(map, LINE_SURE)
            .into_iter()
            .map(|[l, t, r, b]| {
                let x = |v: u32| ((v as f32 * scale.0).round() as u32).min(width);
                let y = |v: u32| ((v as f32 * scale.1).round() as u32).min(height);
                [x(l), y(t), x(r), y(b)]
            })
            .collect();
        give_lines(&mut kept, &lines);
        Ok(kept)
    }
}

/// How sure the detector must be that a pixel is in a line of text.
const LINE_SURE: f32 = 0.3;

/// The boxes of the parts of `map` at least `sure`, each part its pixels
/// touching side by side: left, top, right and bottom (exclusive).
fn components(map: tract_ndarray::ArrayView2<'_, f32>, sure: f32) -> Vec<[u32; 4]> {
    let (h, w) = map.dim();
    let mut seen = vec![false; w * h];
    let mut out = Vec::new();
    let mut stack = Vec::new();
    for y0 in 0..h {
        for x0 in 0..w {
            if seen[y0 * w + x0] || map[[y0, x0]] < sure {
                continue;
            }
            seen[y0 * w + x0] = true;
            stack.push((x0, y0));
            let mut b = [x0, y0, x0, y0];
            while let Some((x, y)) = stack.pop() {
                b = [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)];
                let next = [
                    (x.wrapping_sub(1), y),
                    (x + 1, y),
                    (x, y.wrapping_sub(1)),
                    (x, y + 1),
                ];
                for (nx, ny) in next {
                    if nx < w && ny < h && !seen[ny * w + nx] && map[[ny, nx]] >= sure {
                        seen[ny * w + nx] = true;
                        stack.push((nx, ny));
                    }
                }
            }
            out.push([b[0] as u32, b[1] as u32, b[2] as u32 + 1, b[3] as u32 + 1]);
        }
    }
    out
}

/// Gives each block the lines whose middle is in it, and its writing from
/// them: in columns when as many of them are at least as tall as wide as
/// are wider; a block with none keeps the guess from its shape.
fn give_lines(blocks: &mut [Block], lines: &[[u32; 4]]) {
    for block in blocks {
        let [l, t, r, b] = block.bounds;
        let mut mine: Vec<[u32; 4]> = lines
            .iter()
            .copied()
            .filter(|line| {
                let (cx, cy) = ((line[0] + line[2]) / 2, (line[1] + line[3]) / 2);
                cx >= l && cx < r && cy >= t && cy < b
            })
            .collect();
        // specks the detector marked are no lines: far shorter than the
        // longest, or a few pixels
        let long = |m: &[u32; 4]| (m[2] - m[0]).max(m[3] - m[1]);
        let longest = mine.iter().map(long).max().unwrap_or(0);
        mine.retain(|m| long(m) >= 6 && long(m) * 3 >= longest);
        if mine.is_empty() {
            continue;
        }
        let tall = mine.iter().filter(|m| m[3] - m[1] >= m[2] - m[0]).count();
        // one mark alone may be short columns run together: against the
        // block's shape, the shape decides and the ink is measured instead
        if mine.len() == 1 && (tall == 1) != block.vertical {
            continue;
        }
        block.vertical = tall * 2 >= mine.len();
        if block.vertical {
            mine.sort_by_key(|m| std::cmp::Reverse(m[0] + m[2]));
        } else {
            mine.sort_by_key(|m| m[1] + m[3]);
        }
        block.lines = mine;
    }
}

/// The blocks kept of what the detector gave, centre, size and confidence
/// in its square: the sure ones, each once, on the page, in reading order.
fn blocks(mut found: Vec<([f32; 4], f32)>, scale: (f32, f32), size: (u32, u32)) -> Vec<Block> {
    found.retain(|(b, confidence)| {
        *confidence >= DETECT_CONFIDENCE && b.iter().all(|v| v.is_finite())
    });
    found.sort_by(|a, b| b.1.total_cmp(&a.1));
    let corners = |[cx, cy, w, h]: [f32; 4]| {
        [
            (cx - w / 2.0) * scale.0,
            (cy - h / 2.0) * scale.1,
            (cx + w / 2.0) * scale.0,
            (cy + h / 2.0) * scale.1,
        ]
    };
    let overlap = |a: [f32; 4], b: [f32; 4]| {
        let w = (a[2].min(b[2]) - a[0].max(b[0])).max(0.0);
        let h = (a[3].min(b[3]) - a[1].max(b[1])).max(0.0);
        let area = |r: [f32; 4]| (r[2] - r[0]) * (r[3] - r[1]);
        let union = area(a) + area(b) - w * h;
        if union > 0.0 { w * h / union } else { 0.0 }
    };
    let mut kept: Vec<[f32; 4]> = Vec::new();
    for (b, _) in found {
        let b = corners(b);
        if kept.iter().all(|k| overlap(*k, b) <= DETECT_OVERLAP) {
            kept.push(b);
        }
    }
    let mut out: Vec<Block> = kept
        .into_iter()
        .filter_map(|[l, t, r, b]| {
            let clip = |v: f32, max: u32| (v.max(0.0).round() as u32).min(max);
            let bounds = [
                clip(l, size.0),
                clip(t, size.1),
                clip(r, size.0),
                clip(b, size.1),
            ];
            let (w, h) = (
                bounds[2] - bounds[0].min(bounds[2]),
                bounds[3] - bounds[1].min(bounds[3]),
            );
            // Japanese is written in columns, even in a block somewhat wider
            // than tall: only a clearly wide one is a row of text
            (w >= DETECT_MIN && h >= DETECT_MIN).then_some(Block {
                bounds,
                vertical: w <= 2 * h,
                lines: Vec::new(),
            })
        })
        .collect();
    // from the right, row by row: blocks whose tops are close count as one row
    let row = (size.1 / 8).max(1);
    out.sort_by_key(|b| (b.bounds[1] / row, std::cmp::Reverse(b.bounds[2])));
    out
}

/// About how many characters a block holds, before it is read: its lines
/// (or one) as wide as a character, each as long as the block (MNG-012).
pub fn estimated_chars(block: &Block) -> usize {
    let [l, t, r, b] = block.bounds;
    let (w, h) = (r.saturating_sub(l).max(1), b.saturating_sub(t).max(1));
    let (across, along) = if block.vertical { (w, h) } else { (h, w) };
    let lines = (block.lines.len() as u32).max(1);
    let side = (across / lines).max(1);
    (lines * along / side).max(1) as usize
}

/// Below this grey, a pixel is ink, whatever the ground (the tests' ink).
#[cfg(test)]
const INK: u8 = 128;

/// The grey below which a pixel of `area` of `grey` is ink: well darker than
/// the area's usual grey, its ground, so that the thin grey strokes of a
/// small scan count as well as black ones (MNG-012).
fn ink_below(grey: &image::GrayImage, area: [u32; 4]) -> u8 {
    let (w, h) = grey.dimensions();
    let mut shades = [0u32; 256];
    for y in area[1].min(h)..area[3].min(h) {
        for x in area[0].min(w)..area[2].min(w) {
            shades[usize::from(grey[(x, y)].0[0])] += 1;
        }
    }
    let total: u32 = shades.iter().sum();
    let mut seen = 0;
    let median = shades
        .iter()
        .position(|n| {
            seen += n;
            seen * 2 >= total
        })
        .unwrap_or(255) as u8;
    median.saturating_sub(70).clamp(100, 190)
}

/// How a block's text is drawn on the page (MNG-012).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    /// Its lines: columns from the right (rows from the top when it is not
    /// vertical), each from its first ink to its last, in pixels of the page.
    pub lines: Vec<[u32; 4]>,
    /// What covers its ink: a box for each line with its furigana, and for
    /// the strokes set apart, a pixel wider; the whole block when its lines
    /// are only guessed. Left, top, right and bottom in pixels of the page.
    pub covers: Vec<[u32; 4]>,
}

/// Whether the ink in `m` is a column (or row) of dots: three pieces at
/// least along it, each no longer than about twice its width but the last
/// (a ！ or ？ after them), and each alone, white just outside the band on
/// both sides, where an outline or a stroke crossing it would go on
/// (MNG-012).
fn dotted(grey: &image::GrayImage, below: u8, m: &[u32; 4], vertical: bool) -> bool {
    let ((a0, a1), (g0, g1)) = if vertical {
        ((m[0], m[2]), (m[1], m[3]))
    } else {
        ((m[1], m[3]), (m[0], m[2]))
    };
    let inked = |a: u32, g: u32| {
        let (x, y) = if vertical { (a, g) } else { (g, a) };
        grey.get_pixel_checked(x, y).is_some_and(|p| p.0[0] < below)
    };
    let longest = (a1 - a0) * 2 + 2;
    let mut pieces: Vec<(u32, u32)> = Vec::new();
    for g in g0..g1 {
        if (a0..a1).any(|a| inked(a, g)) {
            match pieces.last_mut() {
                Some(last) if g <= last.1 + 3 => last.1 = g,
                _ => pieces.push((g, g)),
            }
        }
    }
    let alone = |(s, e): (u32, u32)| {
        (s..=e).all(|g| {
            // past the edges' antialiasing, a pixel out
            [a0.saturating_sub(3), a0.saturating_sub(2), a1 + 1, a1 + 2]
                .into_iter()
                .filter(|a| *a + 1 < a0 || *a > a1)
                .all(|a| !inked(a, g))
        })
    };
    let n = pieces.len();
    // the stroke of a ！ or ？ may be followed by its dot
    let long = pieces.iter().filter(|p| p.1 - p.0 >= longest).count();
    n >= 3
        && long <= 1
        && pieces
            .iter()
            .enumerate()
            .all(|(i, p)| (p.1 - p.0 < longest || i + 2 >= n) && alone(*p))
}

/// Two columns (or rows) measured as one line, set apart by a clear white
/// gap across it, as two lines: each at least two fifths of its width and
/// a character and a half long, so that the strokes of い or a line's
/// furigana are not taken for one (MNG-012).
fn split_two(
    grey: &image::GrayImage,
    below: u8,
    m: &[u32; 4],
    vertical: bool,
) -> Option<[[u32; 4]; 2]> {
    let ((a0, a1), (g0, g1)) = if vertical {
        ((m[0], m[2]), (m[1], m[3]))
    } else {
        ((m[1], m[3]), (m[0], m[2]))
    };
    let inked = |a: u32, g: u32| {
        let (x, y) = if vertical { (a, g) } else { (g, a) };
        grey.get_pixel_checked(x, y).is_some_and(|p| p.0[0] < below)
    };
    let counts: Vec<u32> = (a0..a1)
        .map(|a| (g0..g1).filter(|g| inked(a, *g)).count() as u32)
        .collect();
    let most = counts.iter().copied().max().unwrap_or(0);
    let width = a1 - a0;
    // the widest run of near-white across, away from the edges
    let (mut best, mut run) = (None::<(u32, u32)>, None::<u32>);
    for (i, &c) in counts.iter().enumerate() {
        let i = i as u32;
        if c * 20 <= most {
            run.get_or_insert(i);
        } else if let Some(start) = run.take()
            && best.is_none_or(|(s, e)| i - start > e - s)
        {
            best = Some((start, i));
        }
    }
    let (s, e) = best?;
    if e - s < 2 || s * 5 < width * 2 || (width - e) * 5 < width * 2 {
        return None;
    }
    let extent = |p0: u32, p1: u32| -> Option<[u32; 4]> {
        let along: Vec<u32> = (g0..g1)
            .filter(|g| (p0..p1).any(|a| inked(a, *g)))
            .collect();
        let (first, last) = (*along.first()?, *along.last()? + 1);
        let side = p1 - p0;
        if (last - first) * 2 < side * 3 {
            return None;
        }
        Some(if vertical {
            [p0, first, p1, last]
        } else {
            [first, p0, last, p1]
        })
    };
    // columns from the right, rows from the top
    let (near, far) = (extent(a0, a0 + s)?, extent(a0 + e, a1)?);
    Some(if vertical { [far, near] } else { [near, far] })
}

/// `b` a pixel wider each way, on a page `pw` by `ph`.
fn margin(b: [u32; 4], pw: u32, ph: u32) -> [u32; 4] {
    [
        b[0].saturating_sub(1),
        b[1].saturating_sub(1),
        (b[2] + 1).min(pw),
        (b[3] + 1).min(ph),
    ]
}

/// The lines of a block of `chars` characters as the picture draws them,
/// and what covers its ink (MNG-012). Ink is counted across the block and a
/// little beyond it, where furigana may lie; a run of it is a line, runs
/// that together are no wider than a character make one (the strokes of
/// い), runs narrower than half a character are furigana when next to a
/// line, specks otherwise. Without a line that stands out, no lines and the
/// block itself.
pub fn layout(page: &DynamicImage, block: &Block, chars: usize) -> Layout {
    if !block.lines.is_empty() && chars > 0 {
        let vertical = block.vertical;
        let split = Block {
            lines: split_blobs(page, block),
            ..block.clone()
        };
        let block = &split;
        let mut marked = layout_from_lines(page, block);
        if marked.lines.is_empty() {
            return marked;
        }
        let across = |m: &[u32; 4]| if vertical { (m[0], m[2]) } else { (m[1], m[3]) };
        let along = |m: &[u32; 4]| if vertical { (m[1], m[3]) } else { (m[0], m[2]) };
        // a column the detector left unmarked, found in the ink, measured
        // with a character as wide as the lines found: between two marked
        // lines of the same balloon, or beside one inside the block's own
        // box (a first or last column); the art around is left alone
        let line_width = {
            let mut w: Vec<u32> = marked
                .lines
                .iter()
                .map(|m| across(m).1 - across(m).0)
                .collect();
            w.sort_unstable();
            w[w.len() / 2].max(4)
        };
        let [bl, bt, br, bb] = block.bounds;
        let area = u64::from(br.saturating_sub(bl)) * u64::from(bb.saturating_sub(bt));
        let measured = layout(
            page,
            &Block {
                lines: Vec::new(),
                ..block.clone()
            },
            (area / u64::from(line_width * line_width)).max(1) as usize,
        );
        let overlap = |(a0, a1): (u32, u32), (b0, b1): (u32, u32)| {
            a1.min(b1).saturating_sub(a0.max(b0)) * 10 >= (a1 - a0).min(b1 - b0).max(1) * 5
        };
        let mut widths: Vec<u32> = marked
            .lines
            .iter()
            .map(|m| across(m).1 - across(m).0)
            .collect();
        widths.sort_unstable();
        let width = widths[widths.len() / 2].max(1);
        let middle = |m: &[u32; 4]| (across(m).0 + across(m).1) / 2;
        // how much of a box is ink: text is dense, an outline crossing is not
        let grey = page.to_luma8();
        let below = ink_below(&grey, block.bounds);
        let density = |m: &[u32; 4]| {
            let area = u64::from(m[2] - m[0]) * u64::from(m[3] - m[1]);
            let inked = (m[1]..m[3])
                .flat_map(|y| (m[0]..m[2]).map(move |x| (x, y)))
                .filter(|&(x, y)| grey[(x, y)].0[0] < below)
                .count() as u64;
            inked * 1000 / area.max(1)
        };
        let mut dense: Vec<u64> = marked.lines.iter().map(density).collect();
        dense.sort_unstable();
        let dense = dense[dense.len() / 2];
        // the block's own box across, two pixels' slack
        let (b0, b1) = if vertical { (bl, br) } else { (bt, bb) };
        let inside = |m: &[u32; 4]| across(m).0 + 2 >= b0 && across(m).1 <= b1 + 2;
        // what the ink holds apart from the marked lines: its lines, and
        // the narrow runs it took for furigana, which may be a column of
        // dots (covers are a pixel wider than what they cover)
        let found: Vec<[u32; 4]> = measured
            .covers
            .iter()
            .map(|c| {
                [
                    c[0] + 1,
                    c[1] + 1,
                    c[2].saturating_sub(1),
                    c[3].saturating_sub(1),
                ]
            })
            .filter(|c| c[2] > c[0] && c[3] > c[1])
            .collect();
        let missed: Vec<[u32; 4]> = found
            .iter()
            .filter(|m| {
                let w = across(m).1 - across(m).0;
                let length = along(m).1 - along(m).0;
                let beside: Vec<&[u32; 4]> = marked
                    .lines
                    .iter()
                    .filter(|k| overlap(along(k), along(m)))
                    .collect();
                let apart = !marked.lines.iter().any(|k| overlap(across(k), across(m)));
                let between = beside.iter().any(|k| middle(k) < middle(m))
                    && beside.iter().any(|k| middle(k) > middle(m));
                // the nearest line beside it, along any part of it: one
                // column away, no further (furigana touch theirs)
                let gap = marked
                    .lines
                    .iter()
                    .filter(|k| along(k).1.min(along(m).1) > along(k).0.max(along(m).0))
                    .map(|k| {
                        let (k0, k1) = across(k);
                        let (m0, m1) = across(m);
                        if m1 <= k0 {
                            k0 - m1
                        } else {
                            m0.saturating_sub(k1)
                        }
                    })
                    .min();
                // furigana touch their line; a narrow column a third of a
                // column away is text of its own
                let narrow = w * 10 < width * 6;
                let edge = inside(m)
                    && !beside.is_empty()
                    && gap.is_some_and(|g| {
                        g <= width * 2
                            && if narrow {
                                g * 3 >= width
                            } else {
                                g * 4 >= width
                            }
                    })
                    && length >= width;
                apart
                    && ((between
                        && w * 10 >= width * 6
                        && w * 10 <= width * 16
                        && density(m) * 10 >= dense * 6)
                        // a first or last column, of text or of dots: one
                        // thinner than most only as dots, not a stroke or
                        // an outline
                        || (edge
                            && w * 10 <= width * 18
                            && (w * 10 >= width * 6 || dotted(&grey, below, m, vertical))))
            })
            .copied()
            .collect();
        let (pw, ph) = page.dimensions();
        marked
            .covers
            .extend(missed.iter().map(|m| margin(*m, pw, ph)));
        marked.lines.extend(missed);
        if vertical {
            marked.lines.sort_by_key(|m| std::cmp::Reverse(m[0] + m[2]));
        } else {
            marked.lines.sort_by_key(|m| m[1] + m[3]);
        }
        // two columns measured as one, far wider than the others, split
        // where white parts them
        let widths_now: Vec<u32> = marked
            .lines
            .iter()
            .map(|m| across(m).1 - across(m).0)
            .collect();
        let mut sorted = widths_now.clone();
        sorted.sort_unstable();
        let typical = sorted[sorted.len() / 2].max(1);
        let lone = marked.lines.len() == 1;
        let mut lines = Vec::with_capacity(marked.lines.len());
        for m in &marked.lines {
            let w = across(m).1 - across(m).0;
            let long = along(m).1 - along(m).0;
            let wide = (w * 10 >= typical * 14 && !lone) || (lone && w * 2 >= long);
            match wide.then(|| split_two(&grey, below, m, vertical)).flatten() {
                Some(two) => lines.extend(two),
                None => lines.push(*m),
            }
        }
        marked.lines = lines;
        // furigana marked as a line of their own: narrower than most, close
        // after a longer line (to its right in columns, above in rows) that
        // spans them; covered, but not read as text
        let all = marked.lines.clone();
        marked.lines.retain(|m| {
            let (m0, m1) = across(m);
            let (g0, g1) = along(m);
            let narrow = (m1 - m0) * 100 <= width * 80;
            !(narrow
                && all.iter().any(|k| {
                    let (k0, k1) = across(k);
                    let (h0, h1) = along(k);
                    // touching it, as furigana touch their characters
                    let beside = if vertical {
                        k1 <= m0 + 2 && m0 <= k1 + 2
                    } else {
                        m1 <= k0 + 2 && k0 <= m1 + 2
                    };
                    beside && h0 <= g0 + 4 && g1 <= h1 + 4 && (g1 - g0) * 4 <= (h1 - h0) * 3
                }))
        });
        return marked;
    }
    let [l, t, r, b] = block.bounds;
    let (r, b) = (r.min(page.width()), b.min(page.height()));
    let alone = Layout {
        lines: Vec::new(),
        covers: vec![block.bounds],
    };
    if chars == 0 || r <= l || b <= t {
        return alone;
    }
    // a character's side, from the block's area and the characters in it
    let side = (f64::from(r - l) * f64::from(b - t) / chars as f64)
        .sqrt()
        .max(4.0);
    let reach = (side * 0.6) as u32;
    // across the lines, the block and a furigana's width either side
    let (wl, wr, wt, wb) = if block.vertical {
        (l.saturating_sub(reach), (r + reach).min(page.width()), t, b)
    } else {
        (
            l,
            r,
            t.saturating_sub(reach),
            (b + reach).min(page.height()),
        )
    };
    let grey = page.crop_imm(wl, wt, wr - wl, wb - wt).to_luma8();
    let below = ink_below(&grey, [l - wl, t - wt, r - wl, b - wt]);
    let (w, h) = grey.dimensions();
    let (across, along) = if block.vertical { (w, h) } else { (h, w) };
    let ink = |a: u32, g: u32| {
        let (x, y) = if block.vertical { (a, g) } else { (g, a) };
        grey[(x, y)].0[0] < below
    };
    // runs of ink across: where they start and end, and how much ink; an
    // outline or a curve crossing leaves a little ink, no line's
    let counts: Vec<usize> = (0..across)
        .map(|a| (0..along).filter(|g| ink(a, *g)).count())
        .collect();
    let least = ((f64::from(along) * 0.02) as usize)
        .max(counts.iter().copied().max().unwrap_or(0) * 15 / 100)
        .max(1);
    let mut runs: Vec<(u32, u32, usize)> = Vec::new();
    for (a, &count) in (0..across).zip(&counts) {
        if count < least {
            continue;
        }
        match runs.last_mut() {
            Some(run) if a == run.1 + 1 => {
                run.1 = a;
                run.2 += count;
            }
            _ => runs.push((a, a, count)),
        }
    }
    // a line's width: the widest run of ink, a whole column of characters
    // (the count of characters only guessed before they are read, it does
    // not say how wide they are)
    let unit = f64::from(runs.iter().map(|r| r.1 - r.0 + 1).max().unwrap_or(1));
    let narrow = |run: &(u32, u32, usize)| f64::from(run.1 - run.0 + 1) < unit * 0.45;
    // furigana: a narrow run with far less ink than the one beside it
    let ruby = |a: &(u32, u32, usize), b: &(u32, u32, usize)| narrow(a) && a.2 * 10 < b.2 * 3;
    let mut merged: Vec<(u32, u32, usize)> = Vec::new();
    for run in runs {
        match merged.last_mut() {
            // the strokes of one character, much alike and close, make one
            // line
            Some(last)
                if f64::from(run.1 - last.0 + 1) <= unit * 1.25
                    && f64::from(run.0 - last.1) <= unit * 0.35
                    && !ruby(&run, last)
                    && !ruby(last, &run) =>
            {
                last.1 = run.1;
                last.2 += run.2;
            }
            _ => merged.push(run),
        }
    }
    let merged: Vec<(u32, u32)> = merged.into_iter().map(|(a, b, _)| (a, b)).collect();
    // the block's own across range, in the window
    let (inside_start, inside_end) = if block.vertical {
        (l - wl, r - wl)
    } else {
        (t - wt, b - wt)
    };
    let wide = |run: &(u32, u32)| f64::from(run.1 - run.0 + 1) >= unit * 0.45;
    let text: Vec<(u32, u32)> = merged
        .iter()
        .copied()
        .filter(|run| {
            wide(run) && (run.0 + run.1) / 2 >= inside_start && (run.0 + run.1) / 2 < inside_end
        })
        .collect();
    if text.is_empty() {
        return alone;
    }
    // within about two characters: furigana, or a column of dots the
    // caller may take as text (MNG-012)
    let near = |run: &(u32, u32)| {
        text.iter()
            .any(|t| run.0 <= t.1 + reach * 3 && run.1 + reach * 3 >= t.0)
    };
    let furigana: Vec<(u32, u32)> = merged
        .iter()
        .copied()
        .filter(|run| !wide(run) && near(run))
        .collect();
    let extent = |(start, end): (u32, u32)| {
        let inked: Vec<u32> = (0..along)
            .filter(|g| (start..=end).any(|a| ink(a, *g)))
            .collect();
        let (first, last) = (*inked.first()?, *inked.last()?);
        Some(if block.vertical {
            [wl + start, wt + first, wl + end + 1, wt + last + 1]
        } else {
            [wl + first, wt + start, wl + last + 1, wt + end + 1]
        })
    };
    // a line of text has ink along most of it; an outline crossing the
    // block has it only where it crosses, at its ends
    let filled = |(start, end): (u32, u32)| {
        let inked: Vec<u32> = (0..along)
            .filter(|g| (start..=end).any(|a| ink(a, *g)))
            .collect();
        match (inked.first(), inked.last()) {
            (Some(first), Some(last)) => inked.len() * 100 >= (last - first + 1) as usize * 35,
            _ => false,
        }
    };
    let mut lines: Vec<[u32; 4]> = text
        .iter()
        .filter(|run| filled(**run))
        .filter_map(|run| extent(*run))
        .collect();
    // each line and each furigana, a pixel wider for the edges of strokes
    let covers = lines
        .iter()
        .copied()
        .chain(furigana.iter().filter_map(|run| extent(*run)))
        .map(|b| margin(b, page.width(), page.height()))
        .collect();
    if block.vertical {
        lines.reverse();
    }
    Layout { lines, covers }
}

/// The detector's marks, a mark over short columns run together (as wide
/// as a blob, not a line) split into the columns its ink shows.
fn split_blobs(page: &DynamicImage, block: &Block) -> Vec<[u32; 4]> {
    let vertical = block.vertical;
    let across = |m: &[u32; 4]| if vertical { m[2] - m[0] } else { m[3] - m[1] };
    let along = |m: &[u32; 4]| if vertical { m[3] - m[1] } else { m[2] - m[0] };
    let mut widths: Vec<u32> = block.lines.iter().map(across).collect();
    widths.sort_unstable();
    let median = widths.get(widths.len() / 2).copied().unwrap_or(0);
    let (pw, ph) = page.dimensions();
    let mut out = Vec::new();
    for mark in &block.lines {
        let (w, length) = (across(mark), along(mark));
        let blob = w * 10 >= length * 6 || (block.lines.len() >= 2 && w * 10 >= median * 18);
        if blob {
            let pad = w / 4 + 2;
            let sub = Block {
                bounds: [
                    mark[0].saturating_sub(pad),
                    mark[1].saturating_sub(pad),
                    (mark[2] + pad).min(pw),
                    (mark[3] + pad).min(ph),
                ],
                vertical,
                lines: Vec::new(),
            };
            // characters about half as wide as the blob: two columns
            let side = u64::from((w / 2).max(6));
            let [sl, st, sr, sb] = sub.bounds;
            let area = u64::from(sr - sl) * u64::from(sb - st);
            let chars = (area / (side * side)).max(1) as usize;
            let found = layout(page, &sub, chars);
            if found.lines.len() >= 2 {
                // each column's middle half, as thin as a mark
                out.extend(found.lines.iter().map(|line| {
                    let q = across(line) / 4;
                    if vertical {
                        [line[0] + q, line[1], line[2] - q, line[3]]
                    } else {
                        [line[0], line[1] + q, line[2], line[3] - q]
                    }
                }));
                continue;
            }
        }
        out.push(*mark);
    }
    if vertical {
        out.sort_by_key(|m| std::cmp::Reverse(m[0] + m[2]));
    } else {
        out.sort_by_key(|m| m[1] + m[3]);
    }
    out
}

/// The layout from the lines the detector marked (MNG-012): each line's
/// ink measured across a band reaching halfway to its neighbours, the run
/// of ink under the mark its text, the strokes beside it alike in ink part
/// of it, narrow runs with far less ink its furigana; along, the ink from
/// a character before the mark to one after it, inside the block.
fn layout_from_lines(page: &DynamicImage, block: &Block) -> Layout {
    let (pw, ph) = page.dimensions();
    let [l, t, r, b] = block.bounds;
    let (r, b) = (r.min(pw), b.min(ph));
    let grey = page.to_luma8();
    let below = ink_below(&grey, [l, t, r, b]);
    let vertical = block.vertical;
    // across and along a line, in page pixels
    let ink = |a: u32, g: u32| {
        let (x, y) = if vertical { (a, g) } else { (g, a) };
        x < pw && y < ph && grey[(x, y)].0[0] < below
    };
    let mid = |m: &[u32; 4]| {
        if vertical {
            (m[0] + m[2]) / 2
        } else {
            (m[1] + m[3]) / 2
        }
    };
    // a mark may run past the block's box: its line follows it
    let (along_start, along_end) = if vertical { (t, b) } else { (l, r) };
    let along_limit = if vertical { ph } else { pw };
    let limit = if vertical { pw } else { ph };
    let mut lines = Vec::new();
    let mut covers = Vec::new();
    let grow = |part: [u32; 4], cover: &mut [u32; 4]| {
        *cover = [
            cover[0].min(part[0]),
            cover[1].min(part[1]),
            cover[2].max(part[2]),
            cover[3].max(part[3]),
        ];
    };
    let centres: Vec<u32> = block.lines.iter().map(mid).collect();
    // the marks a line was measured for
    let mut marks: Vec<[u32; 4]> = Vec::new();
    for (i, mark) in block.lines.iter().enumerate() {
        let c = centres[i];
        let (mark_start, mark_end) = if vertical {
            (mark[1], mark[3])
        } else {
            (mark[0], mark[2])
        };
        let mark_width = if vertical {
            mark[2] - mark[0]
        } else {
            mark[3] - mark[1]
        };
        // the band: halfway to the nearest line alongside (another balloon
        // above or below is not beside it), and never more than a
        // character and its furigana either way: the mark is about half a
        // character wide
        let alongside = |o: &[u32; 4]| {
            let (o0, o1) = if vertical { (o[1], o[3]) } else { (o[0], o[2]) };
            o1.min(mark_end) > o0.max(mark_start)
        };
        let reach_most = (mark_width * 2).max(6);
        let half = block
            .lines
            .iter()
            .enumerate()
            .filter(|(j, o)| *j != i && alongside(o))
            .map(|(j, _)| centres[j].abs_diff(c) / 2)
            .min()
            .map_or(reach_most, |h| h.clamp(2, reach_most));
        // and inside the block, with room for furigana past its edges
        let (a0, a1) = if vertical { (l, r) } else { (t, b) };
        let pad = (a1 - a0) / 8 + 4;
        let (band_start, band_end) = (
            c.saturating_sub(half).max(a0.saturating_sub(pad)),
            (c + half).min(limit).min(a1 + pad),
        );
        // along, a little past the mark (it is drawn shorter than the
        // text): about a character, judged from the mark's own width
        let reach = (mark_width * 3).max(4);
        let (g0, g1) = (
            mark_start
                .saturating_sub(reach)
                .max(along_start.min(mark_start)),
            (mark_end + reach)
                .min(along_end.max(mark_end))
                .min(along_limit),
        );
        if g1 <= g0 {
            continue;
        }
        // runs of ink across the band; a balloon's outline crossing it
        // leaves a little ink everywhere, which is not a line's
        let counts: Vec<usize> = (band_start..band_end)
            .map(|a| (g0..g1).filter(|g| ink(a, *g)).count())
            .collect();
        let least = (counts.iter().copied().max().unwrap_or(0) * 15 / 100).max(1);
        let mut runs: Vec<(u32, u32, usize)> = Vec::new();
        for (a, &count) in (band_start..band_end).zip(&counts) {
            if count < least {
                continue;
            }
            match runs.last_mut() {
                Some(run) if a <= run.1 + 2 => {
                    run.1 = a;
                    run.2 += count;
                }
                _ => runs.push((a, a, count)),
            }
        }
        let Some(core) = runs
            .iter()
            .position(|run| run.0 <= c && c <= run.1)
            .or_else(|| {
                (0..runs.len()).min_by_key(|k| runs[*k].0.abs_diff(c).min(runs[*k].1.abs_diff(c)))
            })
        else {
            continue;
        };
        // the text: the ink the mark lies on (the mark is drawn inside its
        // line), the runs of it touching the mark's width; a run beside it
        // that is narrow and faint is its furigana, covered with it; the
        // rest of the band, another line or the art, is left alone
        let (m0, m1) = if vertical {
            (mark[0], mark[2])
        } else {
            (mark[1], mark[3])
        };
        let on_mark = |run: &(u32, u32, usize)| run.0 <= m1 && m0 <= run.1 + 1;
        let mut text = runs[core];
        for run in runs.iter().filter(|run| on_mark(run)) {
            text = (text.0.min(run.0), text.1.max(run.1), text.2 + run.2);
        }
        let text = (
            text.0,
            text.1,
            runs.iter()
                .filter(|r| r.0 >= text.0 && r.1 <= text.1)
                .map(|r| r.2)
                .sum::<usize>(),
        );
        let ruby_of = |run: &(u32, u32, usize)| {
            (run.1 - run.0 + 1) * 10 <= (text.1 - text.0 + 1) * 6 && run.2 * 10 < text.2 * 3
        };
        let width = text.1 - text.0 + 1;
        let ruby: Vec<(u32, u32, usize)> = runs
            .iter()
            .filter(|run| run.1 < text.0 || run.0 > text.1)
            .filter(|run| {
                let gap = if run.1 < text.0 {
                    text.0 - run.1
                } else {
                    run.0 - text.1
                };
                gap <= (width * 6 / 10).max(3) && ruby_of(run)
            })
            .copied()
            .collect();
        // along, the ink within about a character of the mark's ends
        let slack = (width * 3 / 4).max(reach);
        let (g0, g1) = (
            mark_start
                .saturating_sub(slack)
                .max(along_start.min(mark_start)),
            (mark_end + slack)
                .min(along_end.max(mark_end))
                .min(along_limit),
        );
        // beyond the mark's ends, a stroke whose ink goes on past the line
        // on both sides crosses it (a balloon's outline, a frame): the line
        // ends before it (AK-BUG-010)
        let crosses = |(a0, a1): (u32, u32), g: u32| {
            a0.checked_sub(3).is_some_and(|a| ink(a, g)) && ink(a1 + 3, g)
        };
        let extent = |(a0, a1): (u32, u32)| {
            let inked: Vec<u32> = (g0..g1).filter(|g| (a0..=a1).any(|a| ink(a, *g))).collect();
            // the inked stretches along the line, gaps of a pixel bridged
            let mut parts: Vec<(u32, u32)> = Vec::new();
            for &g in &inked {
                match parts.last_mut() {
                    Some(part) if g <= part.1 + 2 => part.1 = g,
                    _ => parts.push((g, g)),
                }
            }
            let crossing = |part: &(u32, u32)| {
                (part.0..=part.1).filter(|g| crosses((a0, a1), *g)).count() * 2
                    > (part.1 - part.0 + 1) as usize
            };
            // ink beside the mark but not along it (furigana, dots a
            // little off): all of it, as found
            let on = |part: &(u32, u32)| part.1 >= mark_start && part.0 < mark_end;
            let (first, last) = match (parts.iter().position(on), parts.iter().rposition(on)) {
                (Some(on_mark), Some(last_on_mark)) => (
                    parts[..on_mark]
                        .iter()
                        .rposition(crossing)
                        .map_or(0, |k| k + 1),
                    parts[last_on_mark + 1..]
                        .iter()
                        .position(crossing)
                        .map_or(parts.len() - 1, |k| last_on_mark + k),
                ),
                _ => (0, parts.len().checked_sub(1)?),
            };
            let (first, last) = (parts[first].0, parts[last].1);
            Some(if vertical {
                [a0, first, a1 + 1, last + 1]
            } else {
                [first, a0, last + 1, a1 + 1]
            })
        };
        // the line with its furigana, covered together
        if let Some(line) = extent((text.0, text.1)) {
            let mut cover = line;
            for run in &ruby {
                if let Some(part) = extent((run.0, run.1)) {
                    grow(part, &mut cover);
                }
            }
            covers.push(margin(cover, pw, ph));
            lines.push(line);
            marks.push(*mark);
        }
    }
    // a line far narrower than the others is furigana or a stroke the
    // detector marked apart: covered, not written in
    let across = |m: &[u32; 4]| if vertical { m[2] - m[0] } else { m[3] - m[1] };
    let mut widths: Vec<u32> = lines.iter().map(across).collect();
    widths.sort_unstable();
    if let Some(&median) = widths.get(widths.len() / 2) {
        // apart from the others by a column's gap it is text of its own:
        // dots, or small characters, read and written in
        let span = |m: &[u32; 4]| if vertical { (m[0], m[2]) } else { (m[1], m[3]) };
        let gap = |a: &[u32; 4], b: &[u32; 4]| {
            let ((a0, a1), (b0, b1)) = (span(a), span(b));
            if a1 <= b0 {
                b0 - a1
            } else {
                a0.saturating_sub(b1)
            }
        };
        let kept: Vec<[u32; 4]> = lines
            .iter()
            .zip(&marks)
            .filter(|(m, _)| {
                across(m) * 10 >= median * 6
                    || (dotted(&grey, below, m, vertical)
                        && !lines
                            .iter()
                            .any(|k| k != *m && across(k) > across(m) && gap(k, m) * 3 <= median))
            })
            .map(|(_, mark)| *mark)
            .collect();
        // measured again without them, which narrowed their neighbours'
        // bands, everything found the first time still covered
        if kept.len() < lines.len() && !kept.is_empty() {
            let again = Block {
                lines: kept,
                ..block.clone()
            };
            let mut second = layout_from_lines(page, &again);
            second.covers.extend(covers);
            return second;
        }
    }
    Layout { lines, covers }
}

/// Matrix products on half the processor's cores, as the other readers.
fn half_the_cores() -> tract_linalg::multithread::Executor {
    let cores = std::thread::available_parallelism().map_or(2, |n| n.get());
    tract_linalg::multithread::Executor::multithread((cores / 2).clamp(1, 8))
}

/// The model, loaded.
#[derive(Debug)]
pub struct Engine {
    encoder: Arc<TypedSimplePlan>,
    decoder: Arc<TypedSimplePlan>,
    vocabulary: Vec<String>,
    /// Matrix products on half the processor's cores, as the other readers.
    threads: tract_linalg::multithread::Executor,
}

impl Engine {
    /// Loads the model in `dir`.
    pub fn load(dir: &Path) -> Result<Self, OcrError> {
        Self::load_files(
            &dir.join("encoder.onnx"),
            &dir.join("decoder.onnx"),
            &dir.join("vocab.txt"),
        )
    }

    /// Loads the model from its three files.
    pub fn load_files(
        encoder_file: &Path,
        decoder_file: &Path,
        vocab_file: &Path,
    ) -> Result<Self, OcrError> {
        // the files' annotations name sizes that tract would hold them to
        let onnx = || {
            tract_onnx::onnx()
                .with_ignore_value_info(true)
                .with_ignore_output_shapes(true)
        };
        // pictures read together, as many as given: a batch of a symbolic
        // size, so that the lines of a balloon cost little more than one
        let encoder = onnx().model_for_path(encoder_file).map_err(model_err)?;
        let many = encoder.symbols.sym("N");
        let pixels: TVec<TDim> = tvec!(many.into(), SIDE.into(), SIDE.into(), SIDE.into());
        let pixels: TVec<TDim> = tvec!(pixels[0].clone(), 3.into(), SIDE.into(), SIDE.into());
        let encoder = encoder
            .with_input_fact(0, f32::fact(&pixels).into())
            .and_then(|m| m.into_optimized())
            .and_then(|m| m.into_runnable())
            .map_err(model_err)?;
        // the tokens so far are a symbolic length: the decoder is typed and
        // optimised once, instead of having its shapes worked out again at
        // every token (MNG-003)
        let decoder = onnx().model_for_path(decoder_file).map_err(model_err)?;
        let length = decoder.symbols.sym("L");
        let many = decoder.symbols.sym("N");
        let tokens: TVec<TDim> = tvec!(many.clone().into(), length.into());
        let hidden: TVec<TDim> = tvec!(many.into(), HIDDEN.0.into(), HIDDEN.1.into());
        let decoder = decoder
            .with_input_fact(0, i64::fact(&tokens).into())
            .and_then(|m| m.with_input_fact(1, f32::fact(&hidden).into()))
            .and_then(|m| m.into_optimized())
            .and_then(|m| m.into_runnable())
            .map_err(model_err)?;
        let vocabulary = std::fs::read_to_string(vocab_file)
            .map_err(model_err)?
            .lines()
            .map(str::to_string)
            .collect();
        Ok(Self {
            encoder,
            decoder,
            vocabulary,
            threads: half_the_cores(),
        })
    }

    /// The text in `picture`: one balloon or line, not a page.
    pub fn read(&self, picture: &DynamicImage) -> Result<String, OcrError> {
        tract_linalg::multithread::multithread_tract_scope(self.threads.clone(), || {
            self.read_here(picture)
        })
    }

    /// The same on this thread alone, for callers that already read many
    /// pictures at once on threads of their own (SUB-028).
    pub fn read_alone(&self, picture: &DynamicImage) -> Result<String, OcrError> {
        tract_linalg::multithread::multithread_tract_scope(
            tract_linalg::multithread::Executor::SingleThread,
            || self.read_here(picture),
        )
    }

    fn read_here(&self, picture: &DynamicImage) -> Result<String, OcrError> {
        Ok(self
            .read_together(std::slice::from_ref(picture))?
            .remove(0)
            .text)
    }

    /// The text in each of `pictures`, the lines of a balloon (MNG-012),
    /// read together: one pass of the encoder over them all, then each step
    /// of the decoder for all of them at once, a text that has ended kept
    /// going with its end, so that they all stay as long.
    pub fn read_many(&self, pictures: &[DynamicImage]) -> Result<Vec<String>, OcrError> {
        Ok(self
            .read_many_scored(pictures)?
            .into_iter()
            .map(|r| r.text)
            .collect())
    }

    /// [`Engine::read_many`] with how sure the model was of each character.
    pub fn read_many_scored(&self, pictures: &[DynamicImage]) -> Result<Vec<Reading>, OcrError> {
        tract_linalg::multithread::multithread_tract_scope(self.threads.clone(), || {
            self.read_together(pictures)
        })
    }

    fn read_together(&self, pictures: &[DynamicImage]) -> Result<Vec<Reading>, OcrError> {
        let n = pictures.len();
        if n == 0 {
            return Ok(Vec::new());
        }
        let mut pixels = tract_ndarray::Array4::<f32>::zeros((n, 3, SIDE, SIDE));
        for (i, picture) in pictures.iter().enumerate() {
            let one = pixel_values(picture);
            let one = one.to_plain_array_view::<f32>().map_err(model_err)?;
            pixels
                .index_axis_mut(tract_ndarray::Axis(0), i)
                .assign(&one.index_axis(tract_ndarray::Axis(0), 0));
        }
        let hidden = self
            .encoder
            .run(tvec!(Tensor::from(pixels).into()))
            .map_err(model_err)?
            .remove(0);
        let rows = self.greedy(&hidden, n)?;
        Ok(rows
            .iter()
            .map(|(row, sure)| {
                let end = row.iter().position(|t| *t == END).unwrap_or(row.len());
                // the first token is START, which has no probability
                clean_scored(&row[1..end.max(1)], sure, &self.vocabulary)
            })
            .collect())
    }

    /// The decoder's scores for the last token of each row of `tokens`,
    /// each row reading the encoder's output `pictures[row]`.
    fn step(
        &self,
        hidden: &TValue,
        pictures: &[usize],
        tokens: &[Vec<i64>],
    ) -> Result<Vec<Vec<f32>>, OcrError> {
        let rows = tokens.len();
        let length = tokens.first().map_or(0, Vec::len);
        let flat: Vec<i64> = tokens.iter().flatten().copied().collect();
        let input: Tensor = tract_ndarray::Array2::from_shape_vec((rows, length), flat)
            .map_err(model_err)?
            .into();
        // the encoder's output of each row's picture
        let all = hidden.to_plain_array_view::<f32>().map_err(model_err)?;
        let same = pictures.iter().enumerate().all(|(i, p)| *p == i) && rows == all.shape()[0];
        let hidden: TValue = if same {
            hidden.clone()
        } else {
            let mut picked = tract_ndarray::Array3::<f32>::zeros((rows, HIDDEN.0, HIDDEN.1));
            for (row, picture) in pictures.iter().enumerate() {
                picked
                    .index_axis_mut(tract_ndarray::Axis(0), row)
                    .assign(&all.index_axis(tract_ndarray::Axis(0), *picture));
            }
            Tensor::from(picked).into()
        };
        let output = self
            .decoder
            .run(tvec!(input.into(), hidden))
            .map_err(model_err)?
            .remove(0);
        let logits = output.to_plain_array_view::<f32>().map_err(model_err)?;
        Ok((0..rows)
            .map(|row| {
                logits
                    .index_axis(tract_ndarray::Axis(0), row)
                    .index_axis(tract_ndarray::Axis(0), length - 1)
                    .iter()
                    .copied()
                    .collect()
            })
            .collect())
    }

    /// The likeliest token at each step: each picture's tokens, ended by
    /// [`END`] or cut at [`MAX_TOKENS`].
    fn greedy(&self, hidden: &TValue, n: usize) -> Result<Vec<Tokens>, OcrError> {
        let pictures: Vec<usize> = (0..n).collect();
        let mut tokens: Vec<Vec<i64>> = vec![vec![START]; n];
        let mut sure: Vec<Vec<f32>> = vec![Vec::new(); n];
        let mut ended = vec![false; n];
        while tokens[0].len() < MAX_TOKENS && ended.iter().any(|e| !e) {
            let scores = self.step(hidden, &pictures, &tokens)?;
            for (i, row) in tokens.iter_mut().enumerate() {
                let (next, p) = if ended[i] {
                    (END, 1.0)
                } else {
                    best(&scores[i])
                };
                ended[i] |= next == END;
                row.push(next);
                sure[i].push(p);
            }
        }
        Ok(tokens.into_iter().zip(sure).collect())
    }
}

/// Below this probability a character of a line is unsure, and the line's
/// balloon is read whole to [`reconcile`] it.
pub const UNSURE: f32 = 0.8;

/// The lines of a balloon, read one by one, set right by the same balloon
/// read whole (MNG-012): a line alone is sometimes too narrow for the model
/// to tell its characters apart (かな read as カムー under a long dash),
/// while the whole balloon gives them their context; but the whole reading
/// has its own mistakes. Where the two readings align character for
/// character and differ, the character the model was surer of is kept;
/// characters only one of them has are left as the lines have them, which
/// keeps each line's length and place. Readings that differ by more than a
/// third are not mixed.
pub fn reconcile(lines: &[Reading], whole: &Reading) -> Vec<String> {
    let line_chars: Vec<Vec<char>> = lines.iter().map(|l| l.text.chars().collect()).collect();
    let flat: Vec<char> = line_chars.iter().flatten().copied().collect();
    let flat_sure: Vec<f32> = lines.iter().flat_map(|l| l.sure.iter().copied()).collect();
    let other: Vec<char> = whole.text.chars().collect();
    let unchanged = || lines.iter().map(|l| l.text.clone()).collect();
    if flat.is_empty()
        || other.is_empty()
        || flat == other
        || flat_sure.len() != flat.len()
        || whole.sure.len() != other.len()
    {
        return unchanged();
    }
    // edit distance, then the path back through it
    let (n, m) = (flat.len(), other.len());
    let mut cost = vec![vec![0usize; m + 1]; n + 1];
    for (i, row) in cost.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in cost[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=n {
        for j in 1..=m {
            let same = usize::from(flat[i - 1] != other[j - 1]);
            cost[i][j] = (cost[i - 1][j - 1] + same)
                .min(cost[i - 1][j] + 1)
                .min(cost[i][j - 1] + 1);
        }
    }
    if cost[n][m] * 3 > n.max(m) {
        return unchanged();
    }
    let mut fixed = flat.clone();
    let (mut i, mut j) = (n, m);
    while i > 0 && j > 0 {
        let same = usize::from(flat[i - 1] != other[j - 1]);
        if cost[i][j] == cost[i - 1][j - 1] + same {
            if whole.sure[j - 1] > flat_sure[i - 1] {
                fixed[i - 1] = other[j - 1];
            }
            i -= 1;
            j -= 1;
        } else if cost[i][j] == cost[i - 1][j] + 1 {
            i -= 1;
        } else {
            j -= 1;
        }
    }
    let mut start = 0;
    line_chars
        .iter()
        .map(|line| {
            let end = start + line.len();
            let text = fixed[start..end].iter().collect();
            start = end;
            text
        })
        .collect()
}

/// A text's tokens, from START, and the probability of each after START.
type Tokens = (Vec<i64>, Vec<f32>);

/// The likeliest token and its probability.
fn best(scores: &[f32]) -> (i64, f32) {
    let Some((token, max)) = scores.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)) else {
        return (END, 1.0);
    };
    let sum: f32 = scores.iter().map(|s| (s - max).exp()).sum();
    (token as i64, 1.0 / sum)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// [`full_width`] on a plain text, an ellipsis as three dots.
    fn fw(text: String) -> String {
        let chars: Vec<(char, f32)> = text.replace('…', "...").chars().map(|c| (c, 1.0)).collect();
        full_width(&chars).into_iter().map(|(c, _)| c).collect()
    }

    // T-MNG-003: a selection in shares of the page, kept inside it
    #[test]
    fn a_selection_stays_on_the_page() {
        assert_eq!(
            pixels([0.5, 0.25, 0.5, 0.5], 100, 200),
            Some((50, 50, 50, 100))
        );
        assert_eq!(
            pixels([0.9, 0.9, 5.0, 5.0], 100, 100),
            Some((90, 90, 10, 10))
        );
        assert_eq!(
            pixels([-1.0, 0.0, 0.5, 0.5], 100, 100),
            Some((0, 0, 50, 50))
        );
        assert_eq!(pixels([0.0, 0.0, 0.01, 0.5], 100, 100), None, "too thin");
        assert_eq!(pixels([1.0, 1.0, 0.5, 0.5], 100, 100), None, "off the page");
        assert_eq!(
            pixels([f32::NAN, 0.0, 0.5, 0.5], 100, 100),
            Some((0, 0, 50, 50))
        );
    }

    // T-MNG-003
    #[test]
    fn a_crop_is_of_the_page() {
        let mut page = image::RgbImage::new(40, 20);
        page.put_pixel(30, 10, image::Rgb([255, 0, 0]));
        let mut bytes = Vec::new();
        page.write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
            .unwrap();
        let part = crop(&bytes, [0.5, 0.0, 0.5, 1.0]).unwrap();
        assert_eq!(part.dimensions(), (20, 20));
        assert_eq!(part.to_rgb8()[(10, 10)].0, [255, 0, 0]);
        assert_eq!(
            crop(&bytes, [0.0, 0.0, 0.01, 0.01]),
            Err(OcrError::TooSmall)
        );
        assert!(matches!(crop(b"junk", [0.0; 4]), Err(OcrError::Picture(_))));
        let jpeg = jpeg(&part).unwrap();
        assert_eq!(jpeg[..2], [0xff, 0xd8]);
    }

    // T-MNG-003: grey, square, and in the model's range
    #[test]
    fn what_the_model_sees() {
        let tall = DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            7,
            900,
            image::Rgb([255, 255, 255]),
        ));
        let tensor = pixel_values(&tall);
        assert_eq!(tensor.shape(), [1, 3, SIDE, SIDE]);
        let values = tensor.to_plain_array_view::<f32>().unwrap();
        assert!(values.iter().all(|v| (*v - 1.0).abs() < 1e-6));
    }

    // T-MNG-012: the lines set right by the balloon read whole
    #[test]
    fn the_surer_reading_of_each_character_is_kept() {
        let read = |text: &str, sure: &[f32]| Reading {
            text: text.into(),
            sure: sure.to_vec(),
        };
        // かな under a long dash, read as カムー alone and unsure
        let lines = [
            read("くんない", &[0.9, 0.9, 0.9, 0.9]),
            read("カムー", &[0.4, 0.3, 0.9]),
        ];
        let whole = read("くんないかなー", &[0.9, 0.9, 0.9, 0.9, 0.8, 0.9, 0.9]);
        assert_eq!(reconcile(&lines, &whole), ["くんない", "かなー"]);
        // the whole reading less sure of a character than the line: kept
        let lines = [read("プラスト", &[0.9, 0.9, 0.9, 0.9])];
        let whole = read("ブラスト", &[0.5, 0.9, 0.9, 0.9]);
        assert_eq!(reconcile(&lines, &whole), ["プラスト"]);
        // what only the whole reading has stays out
        let lines = [read("準備", &[0.9; 2]), read("オッケー", &[0.9; 4])];
        let whole = read("準備オッケーです", &[0.99; 8]);
        assert_eq!(reconcile(&lines, &whole), ["準備", "オッケー"]);
        // too different: the lines as they are
        let lines = [read("ああ．．．", &[0.1; 5])];
        assert_eq!(
            reconcile(&lines, &read("．．．．．．", &[0.9; 6])),
            ["ああ．．．"]
        );
        assert_eq!(
            reconcile(&[], &read("何か", &[0.9; 2])),
            Vec::<String>::new()
        );
    }

    // T-MNG-003: each character keeps its token's probability through the
    // clean-up
    #[test]
    fn the_probabilities_follow_the_characters() {
        let mut vocabulary: Vec<String> = (0..SPECIAL).map(|i| format!("<{i}>")).collect();
        vocabulary.extend(["…", "!", " ", "猫"].map(String::from));
        let tokens = [15, 16, 17, 18].map(|t| t as i64);
        let r = clean_scored(&tokens, &[0.5, 0.6, 0.7, 0.8], &vocabulary);
        assert_eq!(r.text, "．．．！猫");
        assert_eq!(r.sure, [0.5, 0.5, 0.5, 0.6, 0.8]);
        assert_eq!(best(&[0.0, 0.0]).1, 0.5);
    }

    // T-MNG-003
    #[test]
    fn the_text_of_the_tokens() {
        let mut vocabulary: Vec<String> = (0..SPECIAL).map(|i| format!("<{i}>")).collect();
        vocabulary.extend(["あ", " ", "い"].map(String::from));
        let tokens = [2, 15, 16, 17, 999, -1];
        assert_eq!(clean(&tokens, &vocabulary), "あい");
    }

    // T-MNG-003, T-SUB-028: manga-ocr's last step
    #[test]
    fn ascii_takes_its_full_width_form() {
        assert_eq!(fw("行くぞ!".into()), "行くぞ！");
        assert_eq!(fw("3D?".into()), "３Ｄ？");
        assert_eq!(fw("え…".into()), "え．．．");
        assert_eq!(fw("あ・・・".into()), "あ．．．");
        assert_eq!(fw("東京・大阪".into()), "東京・大阪");
    }

    // T-MNG-004
    #[test]
    fn the_model_is_there_or_not() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!installed(dir.path()));
        for file in &MODEL {
            let bytes = vec![0u8; if file.name == "vocab.txt" { 3 } else { 0 }];
            std::fs::write(dir.path().join(file.name), bytes).unwrap();
        }
        assert!(!installed(dir.path()), "by their sizes");
        assert!(!detector_installed(dir.path()));
        assert_eq!(model_bytes(), 211_289_531);
        assert_eq!(FILES[3], DETECTOR);
    }

    // T-MNG-011: the sure blocks, once each, on the page, from the right
    #[test]
    fn the_blocks_kept() {
        let found = vec![
            // a column on the left, and the same one again, less sure
            ([100.0, 300.0, 40.0, 200.0], 0.9),
            ([102.0, 302.0, 40.0, 200.0], 0.8),
            // one on the right, at the same height
            ([800.0, 310.0, 60.0, 300.0], 0.7),
            // too unsure
            ([500.0, 500.0, 50.0, 50.0], 0.2),
            // lower down, wide, and past the page's edge
            ([1000.0, 900.0, 200.0, 50.0], 0.95),
            // a speck
            ([10.0, 10.0, 2.0, 2.0], 0.9),
            ([f32::NAN, 10.0, 2.0, 2.0], 0.9),
        ];
        let kept = blocks(found, (2.0, 2.0), (2048, 2048));
        assert_eq!(
            kept,
            vec![
                Block {
                    bounds: [1540, 320, 1660, 920],
                    vertical: true,
                    lines: Vec::new(),
                },
                Block {
                    bounds: [160, 400, 240, 800],
                    vertical: true,
                    lines: Vec::new(),
                },
                Block {
                    bounds: [1800, 1750, 2048, 1850],
                    vertical: false,
                    lines: Vec::new(),
                },
            ]
        );
    }

    // T-MNG-012: the columns of a block as the picture draws them, from the
    // right; furigana covered but not a line, specks neither
    #[test]
    fn the_lines_of_a_block_are_found() {
        let mut page = image::GrayImage::from_pixel(200, 200, image::Luma([255]));
        let mut ink = |x0: u32, y0: u32, x1: u32, y1: u32| {
            for x in x0..x1 {
                for y in y0..y1 {
                    page.put_pixel(x, y, image::Luma([0]));
                }
            }
        };
        // a long column on the right, of two strokes with a gap (い)
        ink(150, 20, 158, 180);
        ink(160, 20, 168, 180);
        // its furigana, thin, just past the block's edge
        ink(171, 30, 174, 60);
        // a short column on the left
        ink(110, 20, 128, 80);
        let page = DynamicImage::ImageLuma8(page);
        let block = Block {
            bounds: [108, 18, 170, 182],
            vertical: true,
            lines: Vec::new(),
        };
        let found = layout(&page, &block, 12);
        assert_eq!(found.lines, vec![[150, 20, 168, 180], [110, 20, 128, 80]]);
        all_ink_covered(&page, &found.covers);
        // the same ink read as rows
        let rows = Block {
            vertical: false,
            ..block.clone()
        };
        assert!(!layout(&page, &rows, 12).lines.is_empty());
        assert_eq!(layout(&page, &block, 0).lines, Vec::<[u32; 4]>::new());
        let blank =
            DynamicImage::ImageLuma8(image::GrayImage::from_pixel(50, 50, image::Luma([255])));
        let empty = Block {
            bounds: [0, 0, 50, 50],
            vertical: true,
            lines: Vec::new(),
        };
        assert_eq!(
            layout(&blank, &empty, 3),
            Layout {
                lines: Vec::new(),
                covers: vec![[0, 0, 50, 50]]
            }
        );
    }

    /// Every pixel of ink is under a cover, and the covers leave most of
    /// the page as it is: not one box over the whole block.
    fn all_ink_covered(page: &DynamicImage, covers: &[[u32; 4]]) {
        let grey = page.to_luma8();
        for (x, y, p) in grey.enumerate_pixels() {
            if p.0[0] < INK {
                assert!(
                    covers
                        .iter()
                        .any(|c| c[0] <= x && x < c[2] && c[1] <= y && y < c[3]),
                    "({x}, {y}) uncovered by {covers:?}"
                );
            }
        }
        let area = grey
            .enumerate_pixels()
            .filter(|(x, y, _)| {
                covers
                    .iter()
                    .any(|c| c[0] <= *x && *x < c[2] && c[1] <= *y && *y < c[3])
            })
            .count();
        assert!(area < 6000, "{area}: {covers:?}");
    }

    // T-MNG-012: the detector's lines: each part of its map a box, given to
    // the block it lies in, which they say is in columns or rows
    #[test]
    fn the_detector_lines_go_to_their_blocks() {
        let mut map = tract_ndarray::Array2::<f32>::zeros((20, 30));
        for y in 2..12 {
            map[[y, 3]] = 0.9;
            map[[y, 8]] = 0.9;
        }
        for x in 20..28 {
            map[[15, x]] = 0.5;
        }
        map[[0, 29]] = 0.1;
        let found = components(map.view(), LINE_SURE);
        assert_eq!(found, vec![[3, 2, 4, 12], [8, 2, 9, 12], [20, 15, 28, 16]]);
        let mut blocks = vec![
            Block {
                bounds: [0, 0, 12, 14],
                vertical: false,
                lines: Vec::new(),
            },
            Block {
                bounds: [18, 13, 30, 18],
                vertical: false,
                lines: Vec::new(),
            },
            Block {
                bounds: [0, 16, 5, 20],
                vertical: true,
                lines: Vec::new(),
            },
        ];
        give_lines(&mut blocks, &found);
        assert!(blocks[0].vertical, "two columns");
        // a lone wide mark in a block taller than wide: short columns run
        // together, the shape decides
        let mut lone = vec![Block {
            bounds: [0, 0, 12, 14],
            vertical: true,
            lines: Vec::new(),
        }];
        give_lines(&mut lone, &[[1, 5, 11, 8]]);
        assert!(lone[0].vertical && lone[0].lines.is_empty());
        // a speck beside a line is not a second line
        let mut specked = vec![Block {
            bounds: [0, 0, 40, 100],
            vertical: true,
            lines: Vec::new(),
        }];
        give_lines(&mut specked, &[[10, 10, 30, 90], [32, 80, 33, 81]]);
        assert_eq!(specked[0].lines, vec![[10, 10, 30, 90]]);
        assert_eq!(blocks[0].lines, vec![[8, 2, 9, 12], [3, 2, 4, 12]]);
        assert!(!blocks[1].vertical, "a row");
        assert!(blocks[2].lines.is_empty() && blocks[2].vertical, "kept");
    }

    // T-MNG-012: from the detector's marks, each column's ink, its strokes
    // alike in ink one line, its furigana covered
    #[test]
    fn the_lines_are_measured_from_the_marks() {
        let mut page = image::GrayImage::from_pixel(200, 200, image::Luma([255]));
        let mut ink = |x0: u32, y0: u32, x1: u32, y1: u32| {
            for x in x0..x1 {
                for y in y0..y1 {
                    page.put_pixel(x, y, image::Luma([0]));
                }
            }
        };
        ink(150, 20, 158, 180);
        ink(160, 20, 168, 180);
        ink(171, 30, 174, 60);
        ink(110, 20, 128, 80);
        // a stroke the detector marked as a line of its own
        ink(135, 100, 137, 140);
        let page = DynamicImage::ImageLuma8(page);
        let block = Block {
            bounds: [108, 18, 170, 182],
            vertical: true,
            lines: vec![
                [155, 30, 164, 170],
                [135, 105, 137, 135],
                [115, 30, 124, 70],
            ],
        };
        let found = layout(&page, &block, 12);
        assert_eq!(found.lines, vec![[150, 20, 168, 180], [110, 20, 128, 80]]);
        all_ink_covered(&page, &found.covers);
    }

    // AK-BUG-010: a balloon's outline crossing a column just past its mark
    // is not part of the column (Kaiji v07 p.44: これで ran up to it)
    #[test]
    fn an_outline_across_a_column_ends_it() {
        let mut page = image::GrayImage::from_pixel(200, 200, image::Luma([255]));
        let mut ink = |x0: u32, y0: u32, x1: u32, y1: u32| {
            for x in x0..x1 {
                for y in y0..y1 {
                    page.put_pixel(x, y, image::Luma([0]));
                }
            }
        };
        // the column, characters with small gaps
        for y in [60, 82, 104, 126, 148] {
            ink(150, y, 168, y + 18);
        }
        // the outline across it, a little before its first character
        ink(100, 48, 199, 50);
        // a character like 一 past the mark's other end: still the line's
        ink(152, 172, 166, 175);
        let page = DynamicImage::ImageLuma8(page);
        let block = Block {
            bounds: [140, 40, 180, 190],
            vertical: true,
            lines: vec![[155, 66, 164, 160]],
        };
        let found = layout(&page, &block, 6);
        assert_eq!(found.lines, vec![[150, 60, 168, 175]]);
    }

    // T-MNG-012: a column the detector left unmarked between two marked
    // ones is added; a frame's line beside the balloon is not
    #[test]
    fn a_missed_column_between_two_is_added() {
        let mut page = image::GrayImage::from_pixel(200, 200, image::Luma([255]));
        let mut ink = |x0: u32, y0: u32, x1: u32, y1: u32| {
            for x in x0..x1 {
                for y in y0..y1 {
                    page.put_pixel(x, y, image::Luma([0]));
                }
            }
        };
        ink(20, 20, 38, 180);
        ink(60, 20, 78, 180);
        ink(100, 20, 118, 180);
        // a panel's frame, inside the block's box
        ink(130, 10, 133, 190);
        let page = DynamicImage::ImageLuma8(page);
        let block = Block {
            bounds: [18, 18, 136, 182],
            vertical: true,
            lines: vec![[105, 30, 114, 170], [25, 30, 34, 170]],
        };
        let found = layout(&page, &block, 24);
        assert_eq!(
            found.lines,
            vec![[100, 20, 118, 180], [60, 20, 78, 180], [20, 20, 38, 180]]
        );
        let framed = |c: &[u32; 4]| c[0] <= 131 && 131 < c[2];
        assert!(!found.covers.iter().any(framed), "{:?}", found.covers);
    }

    // T-MNG-012: a balloon's first column, larger and unmarked, and its last,
    // a column of dots, are lines too, inside the block's box; a dotted
    // line marked apart, a column away, is read rather than taken for
    // furigana (Kaiji volume 1, page 8: 脱税, ・・・・)
    #[test]
    fn a_first_column_and_a_column_of_dots_are_lines() {
        let mut page = image::GrayImage::from_pixel(220, 200, image::Luma([255]));
        let mut ink = |x0: u32, y0: u32, x1: u32, y1: u32| {
            for x in x0..x1 {
                for y in y0..y1 {
                    page.put_pixel(x, y, image::Luma([0]));
                }
            }
        };
        ink(140, 20, 160, 70);
        ink(100, 20, 118, 180);
        ink(60, 20, 78, 180);
        // dots, six of them
        for k in 0..6 {
            ink(30, 30 + k * 20, 34, 34 + k * 20);
        }
        let page = DynamicImage::ImageLuma8(page);
        let block = Block {
            bounds: [28, 18, 162, 182],
            vertical: true,
            lines: vec![[105, 30, 114, 170], [65, 30, 74, 170]],
        };
        let found = layout(&page, &block, 10);
        assert_eq!(
            found.lines,
            vec![
                [140, 20, 160, 70],
                [100, 20, 118, 180],
                [60, 20, 78, 180],
                [30, 30, 34, 134]
            ]
        );
        let grey = page.to_luma8();
        for (x, y, p) in grey.enumerate_pixels() {
            let covered = |c: &[u32; 4]| c[0] <= x && x < c[2] && c[1] <= y && y < c[3];
            assert!(
                p.0[0] > 128 || found.covers.iter().any(covered),
                "({x}, {y})"
            );
        }
        // the dots marked by the detector: kept as a line
        let block = Block {
            lines: vec![[105, 30, 114, 170], [65, 30, 74, 170], [31, 35, 33, 130]],
            ..block
        };
        let found = layout(&page, &block, 10);
        assert!(
            found.lines.contains(&[30, 30, 34, 134]),
            "{:?}",
            found.lines
        );
    }

    // T-MNG-012: two columns the detector marked as one, set apart by white,
    // are two lines; a column of い, its strokes apart, stays one
    // (AK-BUG-009, Kaiji volume 1, page 18: ベンツじゃ / ないと)
    #[test]
    fn two_columns_marked_as_one_are_split() {
        let mut page = image::GrayImage::from_pixel(220, 220, image::Luma([255]));
        let mut ink = |x0: u32, y0: u32, x1: u32, y1: u32| {
            for x in x0..x1 {
                for y in y0..y1 {
                    page.put_pixel(x, y, image::Luma([0]));
                }
            }
        };
        // a long column and a short one, 8 pixels apart, marked as one
        ink(150, 20, 180, 200);
        ink(112, 20, 142, 120);
        // two narrower columns beside them
        ink(70, 20, 90, 200);
        ink(30, 20, 50, 200);
        let page = DynamicImage::ImageLuma8(page);
        let block = Block {
            bounds: [28, 18, 182, 202],
            vertical: true,
            lines: vec![[120, 30, 172, 190], [75, 30, 85, 190], [35, 30, 45, 190]],
        };
        let found = layout(&page, &block, 20);
        assert_eq!(
            found.lines,
            vec![
                [150, 20, 180, 200],
                [112, 20, 142, 120],
                [70, 20, 90, 200],
                [30, 20, 50, 200]
            ]
        );
        // い: two strokes, as long as one character, are not two columns
        let grey = page.to_luma8();
        let mut small = image::GrayImage::from_pixel(60, 60, image::Luma([255]));
        for (x0, x1) in [(10, 18), (28, 36)] {
            for x in x0..x1 {
                for y in 10..40 {
                    small.put_pixel(x, y, image::Luma([0]));
                }
            }
        }
        assert_eq!(split_two(&small, 128, &[10, 10, 36, 40], true), None);
        assert!(split_two(&grey, 128, &[112, 20, 180, 200], true).is_some());
    }

    // T-ERR-001
    #[test]
    fn every_error_has_its_message() {
        let errors = [
            OcrError::Model("r".into()),
            OcrError::Picture("r".into()),
            OcrError::TooSmall,
        ];
        let refs: Vec<&dyn nikomiru_core::Localized> = errors
            .iter()
            .map(|e| e as &dyn nikomiru_core::Localized)
            .collect();
        let i18n = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/i18n");
        let problems = nikomiru_core::error::message_problems(&i18n, &refs);
        assert!(problems.is_empty(), "{problems:#?}");
    }
}
