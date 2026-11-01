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
use tract_onnx::tract_hir::infer::InferenceSimplePlan;

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

/// The size of the whole model.
pub const fn model_bytes() -> u64 {
    MODEL[0].bytes + MODEL[1].bytes + MODEL[2].bytes
}

/// Whether the model is in `dir`, as far as a look can tell: a file is put
/// there only after its checksum is checked.
pub fn installed(dir: &Path) -> bool {
    MODEL
        .iter()
        .all(|file| std::fs::metadata(dir.join(file.name)).is_ok_and(|m| m.len() == file.bytes))
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
    let picture = |e: &dyn std::fmt::Display| OcrError::Picture(e.to_string());
    let mut reader = ImageReader::new(Cursor::new(page))
        .with_guessed_format()
        .map_err(|e| picture(&e))?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(MAX_ALLOC);
    reader.limits(limits);
    let decoded = reader.decode().map_err(|e| picture(&e))?;
    let (width, height) = decoded.dimensions();
    let (x, y, w, h) = pixels(area, width, height).ok_or(OcrError::TooSmall)?;
    Ok(decoded.crop_imm(x, y, w, h))
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
fn clean(tokens: &[i64], vocabulary: &[String]) -> String {
    tokens
        .iter()
        .filter_map(|t| usize::try_from(*t).ok())
        .filter(|t| *t >= SPECIAL)
        .filter_map(|t| vocabulary.get(t))
        .flat_map(|token| token.chars())
        .filter(|c| !c.is_whitespace())
        .collect()
}

/// The model, loaded.
#[derive(Debug)]
pub struct Engine {
    encoder: Arc<TypedSimplePlan>,
    decoder: Arc<InferenceSimplePlan>,
    vocabulary: Vec<String>,
}

impl Engine {
    /// Loads the model in `dir`.
    pub fn load(dir: &Path) -> Result<Self, OcrError> {
        // the files' annotations name sizes that tract would hold them to
        let onnx = || {
            tract_onnx::onnx()
                .with_ignore_value_info(true)
                .with_ignore_output_shapes(true)
        };
        let pixels = tvec!(1, 3, SIDE, SIDE);
        let encoder = onnx()
            .model_for_path(dir.join("encoder.onnx"))
            .and_then(|m| m.with_input_fact(0, f32::fact(&pixels).into()))
            .and_then(|m| m.into_optimized())
            .and_then(|m| m.into_runnable())
            .map_err(model_err)?;
        let hidden = tvec!(1, HIDDEN.0, HIDDEN.1);
        let decoder = onnx()
            .model_for_path(dir.join("decoder.onnx"))
            .and_then(|m| m.with_input_fact(0, InferenceFact::dt(i64::datum_type())))
            .and_then(|m| m.with_input_fact(1, f32::fact(&hidden).into()))
            .and_then(|m| m.into_runnable())
            .map_err(model_err)?;
        let vocabulary = std::fs::read_to_string(dir.join("vocab.txt"))
            .map_err(model_err)?
            .lines()
            .map(str::to_string)
            .collect();
        Ok(Self {
            encoder,
            decoder,
            vocabulary,
        })
    }

    /// The text in `picture`: one balloon or line, not a page.
    pub fn read(&self, picture: &DynamicImage) -> Result<String, OcrError> {
        let hidden = self
            .encoder
            .run(tvec!(pixel_values(picture).into()))
            .map_err(model_err)?
            .remove(0);
        let mut tokens = vec![START];
        while tokens.len() < MAX_TOKENS {
            let length = tokens.len();
            let input: Tensor = tract_ndarray::Array2::from_shape_vec((1, length), tokens.clone())
                .map_err(model_err)?
                .into();
            let output = self
                .decoder
                .run(tvec!(input.into(), hidden.clone()))
                .map_err(model_err)?
                .remove(0);
            let logits = output.to_plain_array_view::<f32>().map_err(model_err)?;
            let last = logits.index_axis(tract_ndarray::Axis(1), length - 1);
            let next = last
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.total_cmp(b.1))
                .map_or(END, |(token, _)| token as i64);
            if next == END {
                break;
            }
            tokens.push(next);
        }
        Ok(clean(&tokens, &self.vocabulary))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    // T-MNG-003
    #[test]
    fn the_text_of_the_tokens() {
        let mut vocabulary: Vec<String> = (0..SPECIAL).map(|i| format!("<{i}>")).collect();
        vocabulary.extend(["あ", " ", "い"].map(String::from));
        let tokens = [2, 15, 16, 17, 999, -1];
        assert_eq!(clean(&tokens, &vocabulary), "あい");
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
        assert_eq!(model_bytes(), 116_619_775);
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
