// SPDX-License-Identifier: GPL-3.0-only

//! Advanced ASS (SUB-019): what changes with time or moves a cue away from
//! plain text, read from override tags into [`Effects`], and what it comes to
//! at a moment, [`Frame`]. Pure maths: the overlay draws it. A cue has one set
//! of effects: libass allows a change in the middle of a line, which only
//! karaoke needs here.

use std::ops::Range;

use crate::cue::Rgb;

/// `\fad(in,out)` or `\fade(a1,a2,a3,t1,t2,t3,t4)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Fade {
    /// In and out, in ms.
    Simple(i64, i64),
    /// Three opacities and the four moments between them.
    Full([f32; 3], [i64; 4]),
}

/// `\move(x1,y1,x2,y2[,t1,t2])`, positions in fractions of the video.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Move {
    /// From.
    pub from: (f64, f64),
    /// To.
    pub to: (f64, f64),
    /// When it starts and ends, from the start of the cue; absent: the whole cue.
    pub time: Option<(i64, i64)>,
}

/// `\clip` or `\iclip`: a rectangle in fractions of the video. A vector clip
/// comes to the rectangle around its points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Clip {
    /// Left, top.
    pub min: (f64, f64),
    /// Right, bottom.
    pub max: (f64, f64),
    /// Only what is outside shows (`\iclip`).
    pub inverse: bool,
}

/// What a `\t` animates.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Targets {
    /// `\frx`, `\fry`, `\frz`, degrees.
    pub rotation: [Option<f32>; 3],
    /// `\fscx`, `\fscy`, as factors.
    pub scale: [Option<f32>; 2],
    /// `\fs`.
    pub size: Option<f32>,
    /// `\c`.
    pub color: Option<Rgb>,
    /// `\alpha`, as opacity.
    pub opacity: Option<f32>,
}

/// `\t([t1,t2,][accel,]…)`.
#[derive(Debug, Clone, PartialEq)]
pub struct Transform {
    /// When, from the start of the cue; absent: the whole cue.
    pub time: Option<(i64, i64)>,
    /// How the change accelerates; 1 is even.
    pub accel: f32,
    /// To what.
    pub to: Targets,
}

/// A syllable of karaoke (`\k`, `\K`, `\kf`, `\ko`).
#[derive(Debug, Clone, PartialEq)]
pub struct Syllable {
    /// Its bytes of `plain_text`.
    pub range: Range<usize>,
    /// When it is sung, from the start of the cue.
    pub time: (i64, i64),
    /// It fills as it is sung (`\K`, `\kf`), rather than all at once.
    pub sweep: bool,
}

/// What a cue has beyond its text.
#[derive(Debug, Clone, PartialEq)]
pub struct Effects {
    /// `\fad`, `\fade`.
    pub fade: Option<Fade>,
    /// `\move`.
    pub movement: Option<Move>,
    /// `\org`, as a fraction of the video.
    pub origin: Option<(f64, f64)>,
    /// `\frx`, `\fry`, `\frz`, degrees.
    pub rotation: [f32; 3],
    /// `\fscx`, `\fscy`, as factors.
    pub scale: [f32; 2],
    /// `\alpha`, as opacity.
    pub opacity: f32,
    /// `\clip`, `\iclip`.
    pub clip: Option<Clip>,
    /// `\t`.
    pub transforms: Vec<Transform>,
    /// Karaoke.
    pub karaoke: Vec<Syllable>,
    /// The unsung colour (`\2c`).
    pub unsung: Option<Rgb>,
}

impl Default for Effects {
    fn default() -> Self {
        Self {
            fade: None,
            movement: None,
            origin: None,
            rotation: [0.0; 3],
            scale: [1.0; 2],
            opacity: 1.0,
            clip: None,
            transforms: Vec::new(),
            karaoke: Vec::new(),
            unsung: None,
        }
    }
}

/// What a cue comes to at a moment.
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    /// Its position, from `\move`.
    pub position: Option<(f64, f64)>,
    /// `\org`.
    pub origin: Option<(f64, f64)>,
    /// Degrees around x, y and z.
    pub rotation: [f32; 3],
    /// Factors along x and y.
    pub scale: [f32; 2],
    /// From 0, invisible, to 1.
    pub opacity: f32,
    /// A new size, and how far along.
    pub size: Option<(f32, f32)>,
    /// A new colour, and how far along.
    pub color: Option<(Rgb, f32)>,
    /// The clip.
    pub clip: Option<Clip>,
}

impl Frame {
    /// Whether it draws other than as plain text in place.
    pub fn transformed(&self) -> bool {
        self.rotation != [0.0; 3] || self.scale != [1.0; 2]
    }
}

fn lerp(from: f32, to: f32, by: f32) -> f32 {
    from + (to - from) * by
}

/// How far along `time` is, from 0 to 1.
fn progress(elapsed: i64, (start, end): (i64, i64)) -> f32 {
    if end <= start {
        return if elapsed >= end { 1.0 } else { 0.0 };
    }
    ((elapsed - start) as f32 / (end - start) as f32).clamp(0.0, 1.0)
}

fn fade(fade: Fade, elapsed: i64, duration: i64) -> f32 {
    match fade {
        Fade::Simple(fade_in, fade_out) => {
            let opening = if fade_in > 0 {
                progress(elapsed, (0, fade_in))
            } else {
                1.0
            };
            let closing = if fade_out > 0 {
                1.0 - progress(elapsed, (duration - fade_out, duration))
            } else {
                1.0
            };
            opening * closing
        }
        Fade::Full([a, b, c], [t1, t2, t3, t4]) => {
            if elapsed < t2 {
                lerp(a, b, progress(elapsed, (t1, t2)))
            } else {
                lerp(b, c, progress(elapsed, (t3, t4)))
            }
        }
    }
}

impl Effects {
    /// The cue `elapsed` ms after its start, in a cue of `duration` ms.
    pub fn at(&self, elapsed: i64, duration: i64) -> Frame {
        let mut frame = Frame {
            position: self.movement.map(|m| {
                let by = f64::from(progress(elapsed, m.time.unwrap_or((0, duration))));
                (
                    m.from.0 + (m.to.0 - m.from.0) * by,
                    m.from.1 + (m.to.1 - m.from.1) * by,
                )
            }),
            origin: self.origin,
            rotation: self.rotation,
            scale: self.scale,
            opacity: self.opacity,
            size: None,
            color: None,
            clip: self.clip,
        };
        for t in &self.transforms {
            let by = progress(elapsed, t.time.unwrap_or((0, duration))).powf(t.accel);
            for (value, target) in frame.rotation.iter_mut().zip(t.to.rotation) {
                if let Some(target) = target {
                    *value = lerp(*value, target, by);
                }
            }
            for (value, target) in frame.scale.iter_mut().zip(t.to.scale) {
                if let Some(target) = target {
                    *value = lerp(*value, target, by);
                }
            }
            if let Some(target) = t.to.opacity {
                frame.opacity = lerp(frame.opacity, target, by);
            }
            if let Some(size) = t.to.size {
                frame.size = Some((size, by));
            }
            if let Some(color) = t.to.color {
                frame.color = Some((color, by));
            }
        }
        if let Some(f) = self.fade {
            frame.opacity *= fade(f, elapsed, duration);
        }
        frame
    }

    /// Whether the byte at `offset` is sung by `elapsed`; `None` outside
    /// karaoke.
    pub fn sung(&self, offset: usize, elapsed: i64) -> Option<bool> {
        let s = self.karaoke.iter().find(|s| s.range.contains(&offset))?;
        if !s.sweep {
            return Some(elapsed >= s.time.0);
        }
        let share = (offset - s.range.start) as f32 / s.range.len().max(1) as f32;
        Some(progress(elapsed, s.time) > share)
    }
}

/// A color between two.
pub fn blend(from: Rgb, to: Rgb, by: f32) -> Rgb {
    let channel = |a: u8, b: u8| lerp(f32::from(a), f32::from(b), by).round() as u8;
    Rgb(
        channel(from.0, to.0),
        channel(from.1, to.1),
        channel(from.2, to.2),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-SUB-019
    #[test]
    fn a_fade_opens_and_closes() {
        let e = Effects {
            fade: Some(Fade::Simple(100, 200)),
            ..Effects::default()
        };
        assert_eq!(e.at(0, 1000).opacity, 0.0);
        assert_eq!(e.at(50, 1000).opacity, 0.5);
        assert_eq!(e.at(500, 1000).opacity, 1.0);
        assert_eq!(e.at(900, 1000).opacity, 0.5);
        assert_eq!(e.at(1000, 1000).opacity, 0.0);
    }

    // T-SUB-019
    #[test]
    fn a_full_fade_passes_its_three_levels() {
        let e = Effects {
            fade: Some(Fade::Full([0.0, 1.0, 0.5], [0, 100, 200, 300])),
            ..Effects::default()
        };
        assert_eq!(e.at(50, 400).opacity, 0.5);
        assert_eq!(e.at(150, 400).opacity, 1.0);
        assert_eq!(e.at(300, 400).opacity, 0.5);
    }

    // T-SUB-019
    #[test]
    fn a_move_goes_from_one_place_to_another_in_its_time() {
        let mut e = Effects {
            movement: Some(Move {
                from: (0.0, 0.0),
                to: (1.0, 0.5),
                time: None,
            }),
            ..Effects::default()
        };
        assert_eq!(e.at(500, 1000).position, Some((0.5, 0.25)));
        e.movement = e.movement.map(|m| Move {
            time: Some((100, 200)),
            ..m
        });
        assert_eq!(e.at(50, 1000).position, Some((0.0, 0.0)));
        assert_eq!(e.at(150, 1000).position, Some((0.5, 0.25)));
        assert_eq!(e.at(900, 1000).position, Some((1.0, 0.5)));
    }

    // T-SUB-019
    #[test]
    fn a_transform_runs_with_its_acceleration() {
        let to = Targets {
            rotation: [None, None, Some(90.0)],
            scale: [Some(2.0), None],
            opacity: Some(0.0),
            size: Some(40.0),
            color: Some(Rgb(255, 0, 0)),
        };
        let mut e = Effects {
            transforms: vec![Transform {
                time: None,
                accel: 1.0,
                to,
            }],
            ..Effects::default()
        };
        let f = e.at(500, 1000);
        assert_eq!(f.rotation[2], 45.0);
        assert_eq!(f.scale, [1.5, 1.0]);
        assert_eq!(f.opacity, 0.5);
        assert_eq!(f.size, Some((40.0, 0.5)));
        assert!(f.transformed());
        e.transforms[0].accel = 2.0;
        assert_eq!(e.at(500, 1000).rotation[2], 22.5);
    }

    // T-SUB-019
    #[test]
    fn karaoke_is_sung_in_its_time() {
        let e = Effects {
            karaoke: vec![
                Syllable {
                    range: 0..3,
                    time: (0, 100),
                    sweep: false,
                },
                Syllable {
                    range: 3..7,
                    time: (100, 200),
                    sweep: true,
                },
            ],
            ..Effects::default()
        };
        assert_eq!(e.sung(0, 0), Some(true));
        assert_eq!(e.sung(3, 50), Some(false));
        assert_eq!(e.sung(3, 150), Some(true));
        assert_eq!(e.sung(6, 150), Some(false));
        assert_eq!(e.sung(6, 199), Some(true));
        assert_eq!(e.sung(9, 0), None);
    }

    #[test]
    fn colours_blend() {
        assert_eq!(blend(Rgb(0, 0, 0), Rgb(255, 100, 0), 0.5), Rgb(128, 50, 0));
    }
}
