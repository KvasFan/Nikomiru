// SPDX-License-Identifier: GPL-3.0-only

//! Thumbnails over the progress bar (PLY-014): the frame under the pointer,
//! made by the application (ffmpeg) one at a time, the newest wish first.
//! The bar is cut into steps, so moving the pointer a little shows the same
//! thumbnail and asks for nothing; each step is made once per file.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;

use crate::dictionaries::Done;
use nikomiru_dict::structured::Pixels;

use crate::popup_view::decode_image;

/// The height of a thumbnail, in pixels.
pub const HEIGHT: u32 = 144;
/// The smallest step of the bar, and the most steps a file is cut into.
const MIN_STEP_MS: i64 = 2_000;
const STEPS: i64 = 300;
/// Thumbnails kept per file.
const KEPT: usize = 120;

/// Makes the frames, from the application.
pub trait ThumbnailService: Send {
    /// The frame of `video` at `at_ms` as an image [`HEIGHT`] pixels high. A
    /// request not yet started is replaced by a newer one, whose `done` is
    /// then the only one called.
    fn frame(&self, video: PathBuf, at_ms: i64, done: Done<Vec<u8>>);
}

/// No thumbnails (tests, or without ffmpeg).
#[derive(Debug, Default)]
pub struct NoThumbnails;

impl ThumbnailService for NoThumbnails {
    fn frame(&self, _video: PathBuf, _at_ms: i64, done: Done<Vec<u8>>) {
        done(Err("no thumbnails".into()));
    }
}

/// The step of the bar `at_ms` falls in, by its start, for a file of
/// `duration_ms`.
pub fn step(duration_ms: i64, at_ms: i64) -> i64 {
    let size = (duration_ms / STEPS).max(MIN_STEP_MS);
    (at_ms.clamp(0, duration_ms.max(0)) / size) * size
}

/// The thumbnails of the file open.
#[derive(Debug, Default)]
pub struct Thumbnails {
    video: Option<PathBuf>,
    /// Changes with the file: older answers are dropped.
    generation: u64,
    made: HashMap<i64, Arc<Pixels>>,
    /// The steps made, oldest first, to forget.
    order: VecDeque<i64>,
    /// The step asked and not answered.
    running: Option<i64>,
    /// The step to ask for next.
    wanted: Option<i64>,
    /// The last step the pointer was over.
    last: Option<i64>,
}

impl Thumbnails {
    /// A file opened (or none): its thumbnails start anew.
    pub fn open(&mut self, video: Option<PathBuf>) {
        *self = Self {
            video,
            generation: self.generation + 1,
            ..Self::default()
        };
    }

    /// The pointer is over step `at_ms` of the bar.
    pub fn want(&mut self, at_ms: i64) {
        self.last = Some(at_ms);
        if self.video.is_some() && !self.made.contains_key(&at_ms) && self.running != Some(at_ms) {
            self.wanted = Some(at_ms);
        }
    }

    /// The frame to ask for: the file, the step and the generation its answer
    /// must carry.
    pub fn take(&mut self) -> Option<(u64, PathBuf, i64)> {
        let at = self.wanted.take()?;
        let video = self.video.clone()?;
        self.running = Some(at);
        Some((self.generation, video, at))
    }

    /// The answer for step `at_ms`. A failure is not asked again until the
    /// pointer leaves the step and comes back.
    pub fn done(&mut self, generation: u64, at_ms: i64, result: Result<Vec<u8>, String>) {
        if generation != self.generation {
            return;
        }
        if self.running == Some(at_ms) {
            self.running = None;
        }
        let pixels = match result.map(|bytes| decode_image(&bytes)) {
            Ok(Some(pixels)) => pixels,
            Ok(None) => {
                tracing::debug!("thumbnail at {at_ms} ms: not an image");
                return;
            }
            Err(e) => {
                tracing::debug!("thumbnail at {at_ms} ms: {e}");
                return;
            }
        };
        if self.made.len() >= KEPT
            && let Some(old) = self.order.pop_front()
        {
            self.made.remove(&old);
        }
        self.order.push_back(at_ms);
        self.made.insert(at_ms, Arc::new(pixels));
    }

    /// The last step the pointer was over, while a file is open.
    pub fn last(&self) -> Option<i64> {
        self.last
    }

    /// The thumbnail of step `at_ms`, with the generation it belongs to.
    pub fn get(&self, at_ms: i64) -> Option<(u64, Arc<Pixels>)> {
        self.made
            .get(&at_ms)
            .map(|p| (self.generation, Arc::clone(p)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png() -> Vec<u8> {
        let image = image::RgbaImage::from_pixel(256, 144, image::Rgba([10, 20, 30, 255]));
        let mut bytes = Vec::new();
        image
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        bytes
    }

    // T-PLY-014
    #[test]
    fn the_bar_is_cut_into_steps() {
        // a 24-minute episode: 300 steps of 4.8 s
        assert_eq!(step(1_440_000, 0), 0);
        assert_eq!(step(1_440_000, 4_799), 0);
        assert_eq!(step(1_440_000, 4_800), 4_800);
        assert_eq!(step(1_440_000, 2_000_000), 1_440_000);
        assert_eq!(step(1_440_000, -5), 0);
        // a short clip: never finer than 2 s
        assert_eq!(step(12_000, 3_999), 2_000);
    }

    // T-PLY-014
    #[test]
    fn the_newest_step_is_asked_once_and_old_answers_dropped() {
        let mut t = Thumbnails::default();
        t.want(0);
        assert_eq!(t.take(), None, "no file open");
        t.open(Some("/v/a.mkv".into()));
        t.want(0);
        t.want(4_800);
        let (generation, video, at) = t.take().unwrap();
        assert_eq!(
            (video.to_str(), at),
            (Some("/v/a.mkv"), 4_800),
            "the newest"
        );
        assert_eq!(t.take(), None);
        t.want(4_800);
        assert_eq!(t.take(), None, "already running");
        t.done(generation, 4_800, Ok(png()));
        let (_, pixels) = t.get(4_800).unwrap();
        assert_eq!(pixels.size, [256, 144]);
        t.want(4_800);
        assert_eq!(t.take(), None, "already made");
        // another file: the answer of the first one comes too late
        t.want(9_600);
        let (old, _, _) = t.take().unwrap();
        t.open(Some("/v/b.mkv".into()));
        t.done(old, 9_600, Ok(png()));
        assert!(t.get(9_600).is_none() && t.get(4_800).is_none());
        // a failure or bytes that are no image: nothing
        t.want(0);
        let (generation, _, _) = t.take().unwrap();
        t.done(generation, 0, Ok(b"not an image".to_vec()));
        assert!(t.get(0).is_none());
        t.want(0);
        assert!(t.take().is_some(), "asked again");
    }

    #[test]
    fn only_so_many_thumbnails_are_kept() {
        let mut t = Thumbnails::default();
        t.open(Some("/v/a.mkv".into()));
        let image = png();
        for i in 0..=KEPT as i64 {
            t.want(i * 2_000);
            let (generation, _, at) = t.take().unwrap();
            t.done(generation, at, Ok(image.clone()));
        }
        assert!(t.get(0).is_none(), "the oldest forgotten");
        assert!(t.get(KEPT as i64 * 2_000).is_some());
    }
}
