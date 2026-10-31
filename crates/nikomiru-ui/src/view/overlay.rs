// SPDX-License-Identifier: GPL-3.0-only

//! The subtitle overlay (SUB-009, SUB-010, SUB-014, POP-001…003): cues over the
//! video, clickable and hoverable per grapheme; readings over their kanji
//! and the words already made into cards underlined (LKP-051, LKP-061).
//!
//! Sizes follow the visible video: 36 px at 1080 by default, ASS sizes in
//! PlayResY units when ASS styles are respected. Each line of a cue is one
//! galley and the rect of each grapheme comes from its glyphs, so a click
//! gives the byte offset of exactly that grapheme. egui has no text effects:
//! the outline, the shadow and the blur are offset copies of the text and
//! soft rectangles, with no GL pass.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

use egui::epaint::text::FontsView;
use egui::epaint::{RectShape, TextShape};
use egui::text::{LayoutJob, TextFormat, TextWrapping};
use egui::{
    Align, Color32, CornerRadius, CursorIcon, FontFamily, FontId, Galley, Id, LayerId, Painter,
    Pos2, Rect, Role, Sense, Stroke, Ui, UiBuilder, Vec2, WidgetInfo, pos2, vec2,
};
use nikomiru_subs::cue::VttLine;
use nikomiru_subs::effects::{Frame, blend};
use nikomiru_subs::{AssStyle, Rgb, RunStyle, TextAlign};

use super::theme;
use crate::popup;
use crate::session::Session;
use crate::subtitles::{Obscure, Shown, Subtitles, blur_sigma};
use crate::ui::Command;
use crate::words::Marks;

/// The selected grapheme and the text the popup matched (POP-002).
const HIGHLIGHT: Color32 = Color32::from_rgba_unmultiplied_const(0x4a, 0x90, 0xe2, 0x66);
/// The underline of the words already made into cards (LKP-061).
const MINED: Color32 = Color32::from_rgb(0x6f, 0xcf, 0x97);
/// The outline without an ASS style.
const OUTLINE: Color32 = Color32::from_rgba_unmultiplied_const(0, 0, 0, 0x99);
/// The soft shadow under each row of text: `#cc000000` spread over the row,
/// which leaves about a third of it where the glyphs are.
const SHADOW: Color32 = Color32::from_black_alpha(0x50);
/// The eight directions of the outline copies.
const AROUND: [(f32, f32); 8] = [
    (-1.0, -1.0),
    (0.0, -1.0),
    (1.0, -1.0),
    (-1.0, 0.0),
    (1.0, 0.0),
    (-1.0, 1.0),
    (0.0, 1.0),
    (1.0, 1.0),
];
/// Copies of the text that make up the blur (SUB-014).
const BLUR_COPIES: usize = 24;
/// The size of the secondary subtitle against the main one (SUB-017).
const SECONDARY_SCALE: f32 = 0.85;

/// What the overlay draws, read from the session every frame.
#[derive(Debug)]
pub(crate) struct Scene<'a> {
    /// The subtitles.
    pub subs: &'a Subtitles,
    /// The display size of the video, 0 while unknown.
    pub video_size: (i64, i64),
    /// `[subtitles] font_size`: pixels for a video 1080 px high.
    pub font_size: f64,
    /// `[subtitles] respect_ass_styles`.
    pub respect_ass: bool,
    /// The obscuring that applies now (the session lifts it on hover and
    /// while a popup is open).
    pub obscure: Obscure,
    /// The secondary subtitle, when one is loaded (SUB-017).
    pub secondary: Option<Secondary<'a>>,
    /// A lookup asked from the keyboard: the cue and the grapheme's offset.
    pub keyboard: Option<(usize, usize)>,
    /// The marks of the main cues shown, by index (LKP-051, LKP-061).
    pub marks: HashMap<usize, Marks>,
}

/// The secondary subtitle as the overlay draws it (SUB-017).
#[derive(Debug)]
pub(crate) struct Secondary<'a> {
    /// Its cues.
    pub subs: &'a Subtitles,
    /// Its obscuring now.
    pub obscure: Obscure,
    /// Above the main subtitle at the bottom, or at the top of the video.
    pub bottom: bool,
}

impl<'a> Scene<'a> {
    fn of(s: &'a Session) -> Self {
        let (font_size, respect_ass) = s.subtitle_style();
        Self {
            subs: &s.subs,
            video_size: s.view.video_size,
            font_size,
            respect_ass,
            obscure: s.obscure_now(),
            secondary: s.secondary.source.is_some().then(|| Secondary {
                subs: &s.secondary,
                obscure: s.secondary_obscure_now(),
                bottom: s.secondary_at_bottom(),
            }),
            keyboard: s.keyboard_lookup(),
            marks: s
                .subs
                .shown(false)
                .filter_map(|c| Some((c.index, s.word_marks(&c.cue.plain_text)?)))
                .collect(),
        }
    }
}

/// What the overlay keeps between frames.
#[derive(Debug, Default)]
pub(crate) struct State {
    /// The pointer was over a cue in the last frame: [`Command::SubtitleHover`]
    /// goes out on changes only.
    hovered: bool,
    /// Where the pointer was in the last frame: a hover lookup goes out when
    /// it moves (the first event of a pointer has no delta).
    pointer: Option<Pos2>,
    /// The subtitles' size in the last frame, in physical pixels: the
    /// interface scale leaves it alone (SET-005).
    pub font_px: f32,
    /// What the last frame drew.
    #[cfg(test)]
    pub drawn: Vec<Drawn>,
}

/// A cue as the last frame drew it.
#[cfg(test)]
#[derive(Debug, Clone)]
pub(crate) struct Drawn {
    pub cue: usize,
    pub rect: Rect,
    /// Byte offset and rect of each grapheme.
    pub graphemes: Vec<(usize, Rect)>,
    /// How many rects are highlighted.
    pub highlighted: usize,
    /// How many readings are drawn over it.
    pub readings: usize,
    /// How many underlines mark words already mined.
    pub mined: usize,
    /// Whether any text was painted.
    pub painted: bool,
    /// A cue of the secondary subtitle.
    pub secondary: bool,
}

pub(crate) fn show(
    ctx: &egui::Context,
    s: &Session,
    state: &mut State,
    bar_height: f32,
    out: &mut Vec<Command>,
) {
    draw(ctx, &Scene::of(s), state, bar_height, out);
}

/// Draws `scene` over the whole window. `bar_height`: the control bar while
/// it is visible, which the bottom cues stay above.
pub(crate) fn draw(
    ctx: &egui::Context,
    scene: &Scene<'_>,
    state: &mut State,
    bar_height: f32,
    out: &mut Vec<Command>,
) {
    let window = ctx.content_rect();
    let video = video_rect(window, scene.video_size);
    let units = units(
        video.height(),
        scene.font_size,
        scene.subs.play_res().map(|(_, y)| y),
    );
    state.font_px = units.font * ctx.pixels_per_point();
    let wrap = video.width().max(4.0 * units.font);
    let secondary_units = Units {
        font: units.font * SECONDARY_SCALE,
        ..units
    };
    let mut cues: Vec<Laid> = ctx.fonts_mut(|fonts| {
        // the secondary first: in a stack it goes above the main subtitle
        let mut all: Vec<Laid> = Vec::new();
        if let Some(sec) = scene.secondary.as_ref().filter(|_| !scene.subs.disabled) {
            for shown in sec.subs.shown(false) {
                let mut laid = lay_out(fonts, &shown, &secondary_units, false, wrap, None);
                laid.place = if sec.bottom {
                    Place::Bottom
                } else {
                    Place::Top
                };
                laid.graphemes.clear();
                laid.secondary = true;
                all.push(laid);
            }
        }
        all.extend(scene.subs.shown(scene.respect_ass).map(|shown| {
            let marks = scene.marks.get(&shown.index);
            lay_out(fonts, &shown, &units, scene.respect_ass, wrap, marks)
        }));
        all
    });
    // drawn bottom, middle, top, then the free ones; by layer and file order
    // inside each group (the order of `shown`)
    cues.sort_by_key(|c| rank(c.place));
    let origins = arrange(&cues, video, window, bar_height, units.unit);

    // the video's layer, above its click area: the list, the bar and the
    // popups are in higher layers and take the pointer first
    let id = Id::unique("subtitle-overlay");
    let ui = Ui::new(
        ctx.clone(),
        id,
        UiBuilder::new()
            .layer_id(LayerId::background())
            .max_rect(window),
    );
    let painter = ui.painter();
    let ppp = ctx.pixels_per_point();
    let (pointer, shift) = ctx.input(|i| (i.pointer.hover_pos(), i.modifiers.shift));
    let moved = pointer.is_some() && pointer != state.pointer;
    state.pointer = pointer;
    #[cfg(test)]
    state.drawn.clear();
    let mut inside = false;
    for (cue, origin) in cues.iter().zip(origins) {
        let offset = origin.to_vec2();
        let rect = Rect::from_min_size(origin, cue.size);
        // the popup is placed next to the whole cue (POP-006)
        let anchor = popup::Rect {
            x: f64::from(rect.min.x),
            y: f64::from(rect.min.y),
            w: f64::from(rect.width()),
            h: f64::from(rect.height()),
        };
        let cue_id = if cue.secondary {
            id.with(("secondary", cue.index))
        } else {
            id.with(cue.index)
        };
        // a lookup from the keyboard opens as a click on its grapheme would
        if let Some((index, at)) = scene.keyboard
            && !cue.secondary
            && index == cue.index
        {
            out.push(Command::GraphemeClicked {
                cue: cue.index,
                offset: at,
                anchor,
            });
        }
        let warp = cue
            .frame
            .as_ref()
            .and_then(|f| Warp::of(f, rect, cue.anchor, video, units.ass));
        let over = ui.interact(
            warp.map_or(rect, |w| w.around(rect)),
            cue_id,
            Sense::hover(),
        );
        over.widget_info(|| WidgetInfo::labeled(Role::Label, true, cue.text));
        inside |= over.contains_pointer();
        let (mode, cue_units) = match &scene.secondary {
            Some(sec) if cue.secondary => (sec.obscure, &secondary_units),
            _ => (scene.obscure, &units),
        };
        // hidden: not drawn, but still hoverable (which reveals it) and clickable
        let painted = mode != Obscure::Hidden;
        if painted {
            for region in clip_regions(cue.frame.as_ref(), video, window) {
                let painter = painter.with_clip_rect(region);
                paint(&painter, cue, offset, mode, cue_units, ppp, warp);
            }
        }
        for hit in &cue.graphemes {
            let target = hit.rect.translate(offset);
            let response = ui.interact(
                warp.map_or(target, |w| w.around(target)),
                id.with((cue.index, hit.offset)),
                Sense::CLICK,
            );
            if response.clicked() {
                out.push(Command::GraphemeClicked {
                    cue: cue.index,
                    offset: hit.offset,
                    anchor,
                });
            }
            if response.hovered() {
                ctx.set_cursor_icon(CursorIcon::PointingHand);
                // lookup on hover, with Shift (POP-003): the session counts the pixels
                if moved && let Some(p) = pointer {
                    out.push(Command::GraphemeHovered {
                        cue: cue.index,
                        offset: hit.offset,
                        x: f64::from(p.x),
                        y: f64::from(p.y),
                        shift,
                        anchor,
                    });
                }
            }
        }
        #[cfg(test)]
        state.drawn.push(Drawn {
            cue: cue.index,
            rect,
            graphemes: cue
                .graphemes
                .iter()
                .map(|g| {
                    let target = g.rect.translate(offset);
                    (g.offset, warp.map_or(target, |w| w.around(target)))
                })
                .collect(),
            highlighted: cue.highlight.len(),
            readings: cue.readings,
            mined: cue.mined.len(),
            painted,
            secondary: cue.secondary,
        });
    }
    // also when the cue under the pointer goes away, or V turns them all off
    if inside != state.hovered {
        state.hovered = inside;
        out.push(Command::SubtitleHover(inside));
        // the obscuring changes with it
        ctx.request_repaint();
    }
}

// ---------------------------------------------------------------------------
// Placement (SUB-009, SUB-010): pure maths, in logical points.

/// The visible video inside `window`: letterboxed and centred, 16:9 while
/// its size is unknown.
pub(crate) fn video_rect(window: Rect, (width, height): (i64, i64)) -> Rect {
    let (w, h) = if width > 0 && height > 0 {
        (width as f32, height as f32)
    } else {
        (16.0, 9.0)
    };
    let fit = (window.width() / w).min(window.height() / h);
    Rect::from_center_size(window.center(), vec2(w * fit, h * fit))
}

/// The scale of the subtitles over a visible video `video_height` high.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Units {
    /// One pixel of a video 1080 px high.
    pub unit: f32,
    /// One pixel of the ASS play area (PlayResY), or `unit` without one.
    pub ass: f32,
    /// The default font size, never below 8.
    pub font: f32,
}

pub(crate) fn units(video_height: f32, font_size: f64, play_res_y: Option<u32>) -> Units {
    let unit = video_height / 1080.0;
    let ass = match play_res_y {
        Some(y) if y > 0 => video_height / y as f32,
        _ => unit,
    };
    Units {
        unit,
        ass,
        font: (font_size as f32 * unit).max(8.0),
    }
}

/// Where a cue goes in the video.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Place {
    /// Stacked upwards above the bottom margin and the control bar.
    Bottom,
    /// Stacked in the middle of the video.
    Middle,
    /// Stacked from the top of the video.
    Top,
    /// At a point (fractions of the video), aligned there by the numpad
    /// anchor: ASS `\pos`, WebVTT `line:<percent>`.
    Point { x: f32, y: f32, anchor: u8 },
    /// On a line of the video counted from the top, or from the bottom when
    /// negative (WebVTT `line:<number>`), at `x` (a fraction of its width).
    Line { x: f32, number: i32 },
}

/// The horizontal alignment of a cue, and of its lines inside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HAlign {
    Left,
    Center,
    Right,
}

/// The numpad anchor of a cue: `\an`, or the style's when respected; bottom
/// centre otherwise.
fn anchor(shown: &Shown<'_>) -> u8 {
    shown.anchor.filter(|a| (1..=9).contains(a)).unwrap_or(2)
}

pub(crate) fn place(shown: &Shown<'_>) -> Place {
    let markup = &shown.cue.markup;
    let moved = shown.frame.as_ref().and_then(|(f, _)| f.position);
    if let Some((x, y)) = moved.or(markup.pos) {
        return Place::Point {
            x: x as f32,
            y: y as f32,
            anchor: anchor(shown),
        };
    }
    if let Some(vtt) = &markup.vtt
        && let Some(line) = vtt.line
    {
        let x = vtt.position.map_or(0.5, |p| p as f32 / 100.0);
        return match line {
            VttLine::Percent(p) => Place::Point {
                x,
                y: p as f32 / 100.0,
                anchor: 8,
            },
            VttLine::Number(number) => Place::Line { x, number },
        };
    }
    match anchor(shown) {
        7..=9 => Place::Top,
        4..=6 => Place::Middle,
        _ => Place::Bottom,
    }
}

pub(crate) fn h_align(shown: &Shown<'_>) -> HAlign {
    if let Some(align) = shown.cue.markup.vtt.as_ref().and_then(|v| v.align) {
        return match align {
            TextAlign::Start | TextAlign::Left => HAlign::Left,
            TextAlign::End | TextAlign::Right => HAlign::Right,
            TextAlign::Center => HAlign::Center,
        };
    }
    match (anchor(shown) - 1) % 3 {
        0 => HAlign::Left,
        2 => HAlign::Right,
        _ => HAlign::Center,
    }
}

/// Where something `width` wide starts in a box `room` wide.
fn align_in(align: HAlign, room: f32, width: f32) -> f32 {
    match align {
        HAlign::Left => 0.0,
        HAlign::Center => (room - width) / 2.0,
        HAlign::Right => room - width,
    }
}

/// The left edge of a stacked cue: centred in the video, or 40 units in
/// from its side.
pub(crate) fn stack_x(video: Rect, align: HAlign, width: f32, unit: f32) -> f32 {
    match align {
        HAlign::Left => video.min.x + 40.0 * unit,
        HAlign::Center => video.min.x + (video.width() - width) / 2.0,
        HAlign::Right => video.max.x - width - 40.0 * unit,
    }
}

/// The bottom edge of the bottom stack: 75 units above the bottom of the
/// video, and always 12 units above the control bar.
pub(crate) fn bottom_edge(video: Rect, window: Rect, bar_height: f32, unit: f32) -> f32 {
    (video.max.y - 75.0 * unit).min(window.max.y - bar_height - 12.0 * unit)
}

/// The top-left corner of a free cue `size` big.
pub(crate) fn free_origin(
    video: Rect,
    place: Place,
    align: HAlign,
    size: Vec2,
    line_height: f32,
) -> Pos2 {
    let left = |x: f32| video.min.x + x * video.width() - align_in(align, 0.0, -size.x);
    match place {
        Place::Point { x, y, anchor } => {
            let point = video.min.y + y * video.height();
            let top = match anchor {
                7..=9 => point,
                4..=6 => point - size.y / 2.0,
                _ => point - size.y,
            };
            pos2(left(x), top)
        }
        Place::Line { x, number } if number >= 0 => {
            pos2(left(x), video.min.y + number as f32 * line_height)
        }
        Place::Line { x, number } => pos2(
            left(x),
            video.max.y + (number + 1) as f32 * line_height - size.y,
        ),
        // not free: placed by `arrange`
        Place::Bottom | Place::Middle | Place::Top => video.min,
    }
}

/// The top-left corner of every cue, in drawing order: the bottom stack
/// grows upwards with the first cue on top, 4 units apart; the middle one is
/// centred; the top one starts 40 units down. Free cues sit at their point.
fn arrange(cues: &[Laid<'_>], video: Rect, window: Rect, bar_height: f32, unit: f32) -> Vec<Pos2> {
    let mut origins = vec![Pos2::ZERO; cues.len()];
    let stack = |group: Place, top: &dyn Fn(f32) -> f32, gap: f32, origins: &mut Vec<Pos2>| {
        let members: Vec<usize> = (0..cues.len())
            .filter(|&i| cues[i].place == group)
            .collect();
        let height: f32 = members.iter().map(|&i| cues[i].size.y).sum::<f32>()
            + gap * members.len().saturating_sub(1) as f32;
        let mut y = top(height);
        for i in members {
            let cue = &cues[i];
            origins[i] = pos2(stack_x(video, cue.align, cue.size.x, unit), y);
            y += cue.size.y + gap;
        }
    };
    let bottom = bottom_edge(video, window, bar_height, unit);
    stack(Place::Bottom, &|h| bottom - h, 4.0 * unit, &mut origins);
    stack(
        Place::Middle,
        &|h| video.center().y - h / 2.0,
        0.0,
        &mut origins,
    );
    stack(
        Place::Top,
        &|_| video.min.y + 40.0 * unit,
        0.0,
        &mut origins,
    );
    for (cue, origin) in cues.iter().zip(origins.iter_mut()) {
        if matches!(cue.place, Place::Point { .. } | Place::Line { .. }) {
            *origin = free_origin(video, cue.place, cue.align, cue.size, cue.line_height);
        }
    }
    origins
}

/// The drawing order of the groups: bottom, middle, top, then the free cues.
fn rank(place: Place) -> u8 {
    match place {
        Place::Bottom => 0,
        Place::Middle => 1,
        Place::Top => 2,
        Place::Point { .. } | Place::Line { .. } => 3,
    }
}

// ---------------------------------------------------------------------------
// Style (SUB-010)

/// How a grapheme looks: its inline style first, then the cue's ASS style
/// (only there when ASS styles are respected). An explicit `\b0` beats a
/// bold style; inline colours and sizes count only when ASS styles are
/// respected. Underline and strike-out come from the inline style only.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Look {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub color: Option<Rgb>,
    /// In the file's units (ASS: PlayResY pixels).
    pub size: Option<f32>,
}

pub(crate) fn look(run: Option<&RunStyle>, style: Option<&AssStyle>, respect: bool) -> Look {
    let inline = |f: fn(&RunStyle) -> Option<bool>| run.and_then(f);
    let positive = |s: &f32| *s > 0.0;
    Look {
        bold: inline(|r| r.bold).unwrap_or(style.is_some_and(|s| s.bold)),
        italic: inline(|r| r.italic).unwrap_or(style.is_some_and(|s| s.italic)),
        underline: inline(|r| r.underline) == Some(true),
        strike: inline(|r| r.strike) == Some(true),
        color: run
            .and_then(|r| r.color)
            .filter(|_| respect)
            .or(style.map(|s| s.primary)),
        size: run
            .and_then(|r| r.size)
            .filter(|_| respect)
            .filter(positive)
            .or(style.map(|s| s.size).filter(positive)),
    }
}

fn color(c: Rgb) -> Color32 {
    Color32::from_rgb(c.0, c.1, c.2)
}

// ---------------------------------------------------------------------------
// Layout

/// A grapheme's place in its cue, and the byte offset a click sends.
#[derive(Debug)]
struct Hit {
    offset: usize,
    rect: Rect,
}

/// A cue laid out, in coordinates relative to its top-left corner.
struct Laid<'a> {
    index: usize,
    text: &'a str,
    place: Place,
    align: HAlign,
    size: Vec2,
    /// The height of its first line (WebVTT line numbers count in it).
    line_height: f32,
    /// The colour of text that sets none, and of the outline.
    fill: Color32,
    outline: Color32,
    /// The galleys of the lines and of the readings, at their origins.
    texts: Vec<(Pos2, Arc<Galley>)>,
    /// The ink of every row of text, for the shadow.
    rows: Vec<Rect>,
    graphemes: Vec<Hit>,
    highlight: Vec<Rect>,
    /// The readings drawn, the file's and the dictionaries'.
    readings: usize,
    /// The underlines of the words already mined (LKP-061).
    mined: Vec<Rect>,
    /// A cue of the secondary subtitle: drawn, never clicked (SUB-017).
    secondary: bool,
    /// What ASS does to it at this moment (SUB-019).
    frame: Option<Frame>,
    /// Its numpad anchor: what it scales and turns around.
    anchor: u8,
}

/// Where a point of a cue goes: scaled around its anchor, then turned around
/// its origin, the two rotations out of the screen seen in perspective, as
/// libass does (SUB-019).
#[derive(Debug, Clone, Copy, PartialEq)]
struct Warp {
    anchor: Pos2,
    origin: Pos2,
    scale: [f32; 2],
    /// Radians around x, y and z.
    turn: [f32; 3],
    /// How far the camera is, in points.
    distance: f32,
}

/// libass's camera, in the script's pixels.
const CAMERA: f32 = 312.5;

impl Warp {
    fn of(frame: &Frame, rect: Rect, anchor: u8, video: Rect, unit: f32) -> Option<Self> {
        if !frame.transformed() {
            return None;
        }
        let x = match anchor {
            1 | 4 | 7 => rect.min.x,
            3 | 6 | 9 => rect.max.x,
            _ => rect.center().x,
        };
        let y = match anchor {
            7..=9 => rect.min.y,
            4..=6 => rect.center().y,
            _ => rect.max.y,
        };
        let anchor = pos2(x, y);
        let origin = frame.origin.map_or(anchor, |(x, y)| {
            pos2(
                video.min.x + x as f32 * video.width(),
                video.min.y + y as f32 * video.height(),
            )
        });
        Some(Self {
            anchor,
            origin,
            scale: frame.scale,
            turn: frame.rotation.map(f32::to_radians),
            distance: CAMERA * unit,
        })
    }

    fn apply(&self, p: Pos2) -> Pos2 {
        let scaled = self.anchor + (p - self.anchor) * vec2(self.scale[0], self.scale[1]);
        let v = scaled - self.origin;
        // in ASS, positive angles turn anticlockwise on a screen whose y grows down
        let (sin, cos) = self.turn[2].sin_cos();
        let (x, y) = (v.x * cos + v.y * sin, v.y * cos - v.x * sin);
        let (sin, cos) = self.turn[0].sin_cos();
        let (y, z) = (y * cos, y * sin);
        let (sin, cos) = self.turn[1].sin_cos();
        let (x, z) = (x * cos + z * sin, z * cos - x * sin);
        let near = self.distance / (self.distance + z).max(1.0);
        self.origin + vec2(x * near, y * near)
    }

    /// The rectangle around `rect` once moved.
    fn around(&self, rect: Rect) -> Rect {
        let corners = [
            rect.left_top(),
            rect.right_top(),
            rect.left_bottom(),
            rect.right_bottom(),
        ];
        Rect::from_points(&corners.map(|c| self.apply(c)))
    }
}

/// Where a cue may show, on the screen: all of it, or inside `\\clip`, or
/// around `\\iclip`.
fn clip_regions(frame: Option<&Frame>, video: Rect, window: Rect) -> Vec<Rect> {
    let Some(clip) = frame.and_then(|f| f.clip) else {
        return vec![window];
    };
    let at = |(x, y): (f64, f64)| {
        pos2(
            video.min.x + x as f32 * video.width(),
            video.min.y + y as f32 * video.height(),
        )
    };
    let inner = Rect::from_min_max(at(clip.min), at(clip.max));
    if !clip.inverse {
        return vec![inner];
    }
    vec![
        Rect::from_min_max(window.min, pos2(window.max.x, inner.min.y)),
        Rect::from_min_max(pos2(window.min.x, inner.max.y), window.max),
        Rect::from_min_max(
            pos2(window.min.x, inner.min.y),
            pos2(inner.min.x, inner.max.y),
        ),
        Rect::from_min_max(
            pos2(inner.max.x, inner.min.y),
            pos2(window.max.x, inner.max.y),
        ),
    ]
}

/// A line being laid out: its job, the byte offset and the chars (in the
/// job's text) of each grapheme, and its readings.
type Line = (LayoutJob, Vec<(usize, Range<usize>)>, Vec<Reading>);

/// A reading over graphemes `first..first + covered` of a line.
struct Reading {
    first: usize,
    covered: usize,
    galley: Arc<Galley>,
}

fn lay_out<'a>(
    fonts: &mut FontsView<'_>,
    shown: &Shown<'a>,
    units: &Units,
    respect: bool,
    wrap: f32,
    marks: Option<&Marks>,
) -> Laid<'a> {
    let frame = shown.frame.as_ref().map(|(f, _)| f);
    let align = h_align(shown);
    let fill = shown.style.map_or(Color32::WHITE, |s| color(s.primary));
    let outline = shown.style.map_or(OUTLINE, |s| color(s.outline_color));
    let halign = match align {
        HAlign::Left => Align::LEFT,
        HAlign::Center => Align::Center,
        HAlign::Right => Align::RIGHT,
    };
    let new_job = || LayoutJob {
        wrap: TextWrapping {
            max_width: wrap,
            ..TextWrapping::default()
        },
        halign,
        ..LayoutJob::default()
    };

    let all = shown.cue.graphemes();
    // the file's ruby wins over the dictionaries' readings (LKP-051)
    let dictionary_readings: &[(Range<usize>, String)] = match marks {
        Some(m) if shown.cue.markup.ruby.is_empty() => &m.readings,
        _ => &[],
    };
    // one job per line; for each grapheme its byte offset and its chars in the line
    let mut lines: Vec<Line> = vec![(new_job(), Vec::new(), Vec::new())];
    for g in &all {
        if g.text == "\n" {
            lines.push((new_job(), Vec::new(), Vec::new()));
            continue;
        }
        let Some((job, graphemes, readings)) = lines.last_mut() else {
            continue;
        };
        let look = look(g.style, shown.style, respect);
        let px = look.size.map_or(units.font, |s| (s * units.ass).max(1.0));
        // SUB-019: `\\t` to another size
        let px = match frame.and_then(|f| f.size) {
            Some((to, by)) => px + ((to * units.ass).max(1.0) - px) * by,
            None => px,
        };
        let family = if look.bold {
            theme::bold()
        } else {
            FontFamily::Proportional
        };
        // text that sets no colour takes the cue's, and its copies the outline's
        let mut ink = look.color.map_or(Color32::PLACEHOLDER, color);
        if let Some((to, by)) = frame.and_then(|f| f.color) {
            let from = look.color.unwrap_or(Rgb(255, 255, 255));
            ink = color(blend(from, to, by));
        }
        // karaoke not yet sung
        if let (Some(effects), Some((_, elapsed))) =
            (shown.cue.markup.effects.as_ref(), shown.frame.as_ref())
            && effects.sung(g.offset, *elapsed) == Some(false)
        {
            let unsung = effects
                .unsung
                .or(shown.style.map(|s| s.secondary))
                .unwrap_or(Rgb(255, 0, 0));
            ink = color(unsung);
        }
        let stroke = |on: bool| {
            if on {
                Stroke::new((px / 16.0).max(1.0), ink)
            } else {
                Stroke::NONE
            }
        };
        let format = TextFormat {
            font_id: FontId::new(px, family),
            color: ink,
            italics: look.italic,
            underline: stroke(look.underline),
            strikethrough: stroke(look.strike),
            ..TextFormat::default()
        };
        let auto = dictionary_readings
            .iter()
            .find(|(range, _)| range.start == g.offset)
            .map(|(range, text)| {
                let covered = all.iter().filter(|h| range.contains(&h.offset)).count();
                (text.as_str(), covered)
            });
        if let Some((reading, covered)) = g.ruby.or(auto) {
            // WebVTT ruby: half size, the colour of its base
            let galley = fonts.layout_job(LayoutJob::single_section(
                reading.to_string(),
                TextFormat::simple(FontId::proportional(px / 2.0), ink),
            ));
            readings.push(Reading {
                first: graphemes.len(),
                covered,
                galley,
            });
        }
        let start = job.text.chars().count();
        job.append(g.text, 0.0, format);
        graphemes.push((g.offset, start..start + g.text.chars().count()));
    }

    let mut laid = Laid {
        index: shown.index,
        text: &shown.cue.plain_text,
        place: place(shown),
        align,
        size: Vec2::ZERO,
        line_height: 0.0,
        fill,
        outline,
        texts: Vec::new(),
        rows: Vec::new(),
        graphemes: Vec::new(),
        highlight: Vec::new(),
        readings: 0,
        mined: Vec::new(),
        secondary: false,
        frame: frame.cloned(),
        anchor: anchor(shown),
    };
    // lines with nothing in them take no room, as in a column of rows
    let galleys: Vec<_> = lines
        .into_iter()
        .filter(|(job, _, _)| !job.text.is_empty())
        .map(|(job, graphemes, readings)| (fonts.layout_job(job), graphemes, readings))
        .collect();
    let width = galleys
        .iter()
        .map(|(g, _, _)| g.size().x)
        .fold(0.0, f32::max);
    let mut y = 0.0;
    for (galley, graphemes, readings) in galleys {
        // room for the readings above the line, inside the cue
        let ruby = readings
            .iter()
            .map(|r| r.galley.size().y)
            .fold(0.0, f32::max);
        let origin = pos2(
            align_in(align, width, galley.size().x) - galley.rect.min.x,
            y + ruby,
        );
        if laid.line_height == 0.0 {
            laid.line_height = galley.rows.first().map_or(0.0, |r| r.size.y);
        }
        let first = laid.graphemes.len();
        for (offset, chars) in graphemes {
            let rect = grapheme_rect(&galley, chars)
                .map_or(Rect::NOTHING, |r| r.translate(origin.to_vec2()));
            if shown
                .highlight
                .as_ref()
                .is_some_and(|h| h.contains(&offset))
                && rect.is_positive()
            {
                laid.highlight.push(rect);
            }
            laid.graphemes.push(Hit { offset, rect });
        }
        for reading in readings {
            let covered = &laid.graphemes[first + reading.first..]
                .iter()
                .take(reading.covered.max(1))
                .map(|h| h.rect)
                .filter(|r| r.is_positive())
                .collect::<Vec<_>>();
            let Some(base) = covered.first().copied() else {
                continue;
            };
            // the base on the reading's row only
            let span = covered
                .iter()
                .filter(|r| (r.min.y - base.min.y).abs() < 0.5)
                .fold(base, |a, r| a.union(*r));
            let size = reading.galley.size();
            let at = pos2(
                span.center().x - size.x / 2.0 - reading.galley.rect.min.x,
                span.min.y - size.y,
            );
            laid.texts.push((at, reading.galley));
            laid.readings += 1;
        }
        for row in &galley.rows {
            let ink = row.visuals.mesh_bounds;
            if ink.is_positive() {
                laid.rows
                    .push(ink.translate(origin.to_vec2() + row.pos.to_vec2()));
            }
        }
        y += ruby + galley.size().y;
        laid.texts.push((origin, galley));
    }
    laid.size = vec2(width, y);
    if laid.line_height == 0.0 {
        laid.line_height = units.font;
    }
    // drop the targets of graphemes with no glyph (a lone combining mark)
    laid.graphemes.retain(|h| h.rect.is_positive());
    // a line under each word already mined, one per row it is on
    for range in marks.map_or(&[][..], |m| &m.mined) {
        let mut rows: Vec<Rect> = Vec::new();
        for hit in laid.graphemes.iter().filter(|h| range.contains(&h.offset)) {
            match rows
                .iter_mut()
                .find(|r| (r.max.y - hit.rect.max.y).abs() < 0.5)
            {
                Some(row) => *row = row.union(hit.rect),
                None => rows.push(hit.rect),
            }
        }
        let thick = (units.font / 14.0).max(2.0);
        laid.mined.extend(
            rows.into_iter()
                .map(|r| Rect::from_min_max(pos2(r.min.x, r.max.y - thick), r.max)),
        );
    }
    laid
}

/// The rect of the grapheme made of `chars` (char indices of the galley's
/// text): from its first glyph to the end of its widest one, as high as its
/// row. `None` when it has no glyph.
fn grapheme_rect(galley: &Galley, chars: Range<usize>) -> Option<Rect> {
    let mut start = 0;
    for row in &galley.rows {
        let count = row.glyphs.len();
        if chars.start < start + count {
            let glyphs = row
                .glyphs
                .get(chars.start - start..chars.end.min(start + count) - start)?;
            let left = glyphs.first()?.pos.x;
            let right = glyphs.iter().map(|g| g.max_x()).fold(left, f32::max);
            return Some(Rect::from_min_max(
                row.pos + vec2(left, 0.0),
                row.pos + vec2(right, row.size.y),
            ));
        }
        start += count + usize::from(row.ends_with_newline);
    }
    None
}

// ---------------------------------------------------------------------------
// Painting

/// How the text of a cue with effects goes on the screen (SUB-019).
#[derive(Clone, Copy)]
struct Ink {
    opacity: f32,
    warp: Option<Warp>,
    fill: Color32,
}

/// `galley` at `at`, in `color` if given, else in its own colours. Moved
/// glyph by glyph when it has to be: a text shape only turns as a whole.
fn draw_text(painter: &Painter, at: Pos2, galley: &Arc<Galley>, color: Option<Color32>, ink: Ink) {
    if ink.warp.is_none() && ink.opacity >= 1.0 {
        let mut shape = TextShape::new(at, Arc::clone(galley), color.unwrap_or(ink.fill));
        shape.override_text_color = color;
        painter.add(shape);
        return;
    }
    for row in &galley.rows {
        let mut mesh = row.visuals.mesh.clone();
        if mesh.is_empty() {
            continue;
        }
        let shift = at.to_vec2() + row.pos.to_vec2();
        for vertex in &mut mesh.vertices {
            let own = if vertex.color == Color32::PLACEHOLDER {
                ink.fill
            } else {
                vertex.color
            };
            vertex.color = color.unwrap_or(own).gamma_multiply(ink.opacity);
            let place = vertex.pos + shift;
            vertex.pos = ink.warp.map_or(place, |w| w.apply(place));
        }
        painter.add(mesh);
    }
}

/// The highlight under the text, then the shadow, the outline and the text;
/// or, blurred, a smear of translucent copies (SUB-014).
fn paint(
    painter: &Painter,
    cue: &Laid<'_>,
    offset: Vec2,
    mode: Obscure,
    units: &Units,
    ppp: f32,
    warp: Option<Warp>,
) {
    let opacity = cue.frame.as_ref().map_or(1.0, |f| f.opacity);
    let ink = Ink {
        opacity,
        warp,
        fill: cue.fill,
    };
    let moved = |rect: Rect| warp.map_or(rect, |w| w.around(rect));
    for rect in &cue.highlight {
        painter.rect_filled(moved(rect.translate(offset)), 3.0, HIGHLIGHT);
    }
    // the shadow: blurred by half of the blur's reach, 1 px down
    let sigma = blur_sigma(f64::from(units.font)) as f32;
    let reach = (2.0 * sigma).round().min(64.0);
    // a turned cue has no box around its ink
    for row in cue.rows.iter().filter(|_| warp.is_none()) {
        painter.add(
            RectShape::filled(
                row.translate(offset + vec2(0.0, 1.0)),
                CornerRadius::same(4),
                SHADOW.gamma_multiply(opacity),
            )
            .with_blur_width(reach / 2.0),
        );
    }
    if mode == Obscure::Blur {
        // copies spread over the blur's reach (golden-angle spiral), each faint
        let golden = std::f32::consts::PI * (3.0 - 5f32.sqrt());
        for k in 0..BLUR_COPIES {
            let r = reach / 2.0 * ((k as f32 + 0.5) / BLUR_COPIES as f32).sqrt();
            let a = k as f32 * golden;
            let shift = offset + vec2(r * a.cos(), r * a.sin());
            for (at, galley) in &cue.texts {
                let faint = Some(cue.fill.gamma_multiply(0.12));
                draw_text(painter, *at + shift, galley, faint, ink);
            }
        }
        return;
    }
    // the outline: eight copies one pixel (more on big text) around
    let width = (units.font * ppp / 24.0).round().max(1.0) / ppp;
    for (dx, dy) in AROUND {
        let shift = offset + vec2(dx, dy) * width;
        for (at, galley) in &cue.texts {
            draw_text(painter, *at + shift, galley, Some(cue.outline), ink);
        }
    }
    for (at, galley) in &cue.texts {
        draw_text(painter, *at + offset, galley, None, ink);
    }
    for line in &cue.mined {
        let line = moved(line.translate(offset));
        painter.rect_filled(line.expand(width), 1.0, cue.outline);
        painter.rect_filled(line, 1.0, MINED);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subtitles::{Loaded, Source};
    use egui_kittest::Harness;
    use nikomiru_subs::Format;

    fn window() -> Rect {
        Rect::from_min_size(Pos2::ZERO, vec2(1280.0, 720.0))
    }

    fn subtitles(text: &str, format: Format, at_ms: i64) -> Subtitles {
        let mut subs = Subtitles::default();
        subs.set_track(
            Source::File("a".into()),
            Loaded {
                track: nikomiru_subs::parse(format, text),
                encoding: None,
            },
        );
        subs.update(at_ms);
        subs
    }

    #[test]
    fn the_video_is_letterboxed_and_16_9_while_unknown() {
        // a 4:3 video in a 16:9 window: bars at the sides
        let v = video_rect(window(), (640, 480));
        assert_eq!(v, Rect::from_min_max(pos2(160.0, 0.0), pos2(1120.0, 720.0)));
        // a 2.39:1 video: bars above and below
        let v = video_rect(window(), (1920, 804));
        assert!((v.width() - 1280.0).abs() < 0.01 && (v.center().y - 360.0).abs() < 0.01);
        assert!((v.height() - 536.0).abs() < 0.1, "{v:?}");
        // unknown: 16:9, the whole window
        assert_eq!(video_rect(window(), (0, 0)), window());
        assert_eq!(video_rect(window(), (1920, 0)), window());
    }

    #[test]
    fn sizes_follow_the_visible_video() {
        let u = units(720.0, 36.0, None);
        assert_eq!(u.unit, 720.0 / 1080.0);
        assert_eq!(u.font, 24.0);
        assert_eq!(u.ass, u.unit, "no PlayResY");
        assert_eq!(units(720.0, 36.0, Some(360)).ass, 2.0);
        assert_eq!(units(720.0, 36.0, Some(0)).ass, u.unit);
        // never below 8 px
        assert_eq!(units(100.0, 36.0, None).font, 8.0);
    }

    #[test]
    fn the_bottom_stack_stays_above_the_margin_and_the_bar() {
        let unit = 720.0 / 1080.0;
        let video = window();
        // no bar: 75 units above the bottom of the video
        assert!((bottom_edge(video, window(), 0.0, unit) - 670.0).abs() < 0.01);
        // a bar 104 high: 12 units above it
        assert!((bottom_edge(video, window(), 104.0, unit) - 608.0).abs() < 0.01);
        // a letterboxed video: the margin is from the video, the bar from the window
        let wide = video_rect(window(), (1920, 804));
        assert!((bottom_edge(wide, window(), 0.0, unit) - (wide.max.y - 50.0)).abs() < 0.01);
        assert_eq!(
            stack_x(video, HAlign::Center, 100.0, unit),
            590.0,
            "centred"
        );
        assert!((stack_x(video, HAlign::Left, 100.0, unit) - 26.666_668).abs() < 0.01);
        assert!(
            (stack_x(video, HAlign::Right, 100.0, unit) - (1280.0 - 100.0 - 26.666_668)).abs()
                < 0.01
        );
    }

    #[test]
    fn free_cues_sit_at_their_point() {
        let video = window();
        let size = vec2(100.0, 40.0);
        // \pos(0.5, 0.5) with \an5: centred on the point
        let p = Place::Point {
            x: 0.5,
            y: 0.5,
            anchor: 5,
        };
        assert_eq!(
            free_origin(video, p, HAlign::Center, size, 30.0),
            pos2(590.0, 340.0)
        );
        // \an7: the top-left corner at the point; \an3: the bottom-right one
        let p = Place::Point {
            x: 0.25,
            y: 0.5,
            anchor: 7,
        };
        assert_eq!(
            free_origin(video, p, HAlign::Left, size, 30.0),
            pos2(320.0, 360.0)
        );
        let p = Place::Point {
            x: 0.25,
            y: 0.5,
            anchor: 3,
        };
        assert_eq!(
            free_origin(video, p, HAlign::Right, size, 30.0),
            pos2(220.0, 320.0)
        );
        // WebVTT line numbers: from the top, or from the bottom when negative
        let line = |number| Place::Line { x: 0.5, number };
        assert_eq!(
            free_origin(video, line(0), HAlign::Center, size, 30.0).y,
            0.0
        );
        assert_eq!(
            free_origin(video, line(2), HAlign::Center, size, 30.0).y,
            60.0
        );
        assert_eq!(
            free_origin(video, line(-1), HAlign::Center, size, 30.0).y,
            680.0
        );
        assert_eq!(
            free_origin(video, line(-2), HAlign::Center, size, 30.0).y,
            650.0
        );
    }

    #[test]
    fn groups_and_alignment_from_anchors_positions_and_webvtt() {
        let srt = subtitles(
            "1\n00:00:01,000 --> 00:00:03,000\n{\\an8}上\n\n2\n00:00:01,000 --> 00:00:03,000\n{\\an4}左\n\n3\n00:00:01,000 --> 00:00:03,000\n下\n",
            Format::Srt,
            1500,
        );
        let shown: Vec<_> = srt.shown(false).collect();
        assert_eq!(place(&shown[0]), Place::Top);
        assert_eq!(h_align(&shown[0]), HAlign::Center);
        assert_eq!(place(&shown[1]), Place::Middle);
        assert_eq!(h_align(&shown[1]), HAlign::Left);
        assert_eq!(place(&shown[2]), Place::Bottom);

        let vtt = subtitles(
            "WEBVTT\n\n00:01.000 --> 00:03.000 line:10% position:20% align:start\n上\n\n00:01.000 --> 00:03.000 line:-1 align:end\n下\n\n00:01.000 --> 00:03.000 align:end\n右\n",
            Format::Vtt,
            1500,
        );
        let shown: Vec<_> = vtt.shown(false).collect();
        assert_eq!(
            place(&shown[0]),
            Place::Point {
                x: 0.2,
                y: 0.1,
                anchor: 8
            }
        );
        assert_eq!(h_align(&shown[0]), HAlign::Left);
        assert_eq!(place(&shown[1]), Place::Line { x: 0.5, number: -1 });
        assert_eq!(h_align(&shown[1]), HAlign::Right);
        // without a line, at the bottom, aligned as asked
        assert_eq!(place(&shown[2]), Place::Bottom);
        assert_eq!(h_align(&shown[2]), HAlign::Right);
    }

    // T-SUB-010
    #[test]
    fn inline_style_beats_the_ass_style_only_where_it_should() {
        let ass = "[Script Info]\nPlayResY: 1080\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Sign,Serif,48,&H0000D7FF,&H0,&H0,&H0,-1,0,0,0,100,100,0,0,1,2,0,8,10,10,30,1\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:01.00,0:00:02.00,Sign,,0,0,0,,{\\b0\\fs60\\c&HFF0000&\\u1}青{\\r}看板\n";
        let subs = subtitles(ass, Format::Ass, 1500);
        let cue = subs.cue(0).unwrap();
        let g = cue.graphemes();
        let style = subs.shown(true).next().unwrap().style;
        assert!(style.is_some());
        // respected: \b0 beats the bold style, the inline size and colour win
        let first = look(g[0].style, style, true);
        assert!(!first.bold && first.underline && !first.strike);
        assert_eq!(first.color, Some(Rgb(0, 0, 0xFF)));
        assert_eq!(first.size, Some(60.0));
        // after \r: the style's
        let second = look(g[1].style, style, true);
        assert!(second.bold && !second.underline);
        assert_eq!(second.color, Some(Rgb(0xFF, 0xD7, 0)));
        assert_eq!(second.size, Some(48.0));
        // not respected: no style, no inline colour or size; \b0 and \u still count
        let plain = look(g[0].style, None, false);
        assert!(!plain.bold && plain.underline);
        assert_eq!((plain.color, plain.size), (None, None));
    }

    /// A harness drawing `subs`, with a click area under it like the video's.
    fn harness(
        subs: Subtitles,
        obscure: Obscure,
    ) -> Harness<'static, (Subtitles, State, Vec<Command>)> {
        with_secondary(subs, obscure, None)
    }

    /// The same, with a secondary subtitle: its cues, its obscuring, at the
    /// bottom or not.
    fn with_secondary(
        subs: Subtitles,
        obscure: Obscure,
        secondary: Option<(Subtitles, Obscure, bool)>,
    ) -> Harness<'static, (Subtitles, State, Vec<Command>)> {
        build(subs, obscure, secondary, HashMap::new())
    }

    /// The same, with the words' marks of each cue (LKP-051, LKP-061).
    fn build(
        subs: Subtitles,
        obscure: Obscure,
        secondary: Option<(Subtitles, Obscure, bool)>,
        marks: HashMap<usize, Marks>,
    ) -> Harness<'static, (Subtitles, State, Vec<Command>)> {
        let marks: &'static HashMap<usize, Marks> = Box::leak(Box::new(marks));
        let secondary: &'static Option<(Subtitles, Obscure, bool)> = Box::leak(Box::new(secondary));
        Harness::builder()
            .with_size(vec2(1280.0, 720.0))
            .allow_missing_glyphs()
            .build_ui_state(
                move |ui, (subs, state, out)| {
                    let video = ui.interact(ui.max_rect(), Id::unique("video"), Sense::click());
                    if video.clicked() {
                        out.push(Command::VideoClicked);
                    }
                    let scene = Scene {
                        subs,
                        video_size: (1920, 1080),
                        font_size: 36.0,
                        respect_ass: false,
                        obscure,
                        secondary: secondary.as_ref().map(|(s, o, bottom)| Secondary {
                            subs: s,
                            obscure: *o,
                            bottom: *bottom,
                        }),
                        keyboard: None,
                        marks: marks.clone(),
                    };
                    draw(ui.ctx(), &scene, state, 0.0, out);
                },
                (subs, State::default(), Vec::new()),
            )
    }

    /// A cue of an ASS script, with ASS styles respected, drawn 1.5 s in.
    fn ass_harness(text: &str) -> Harness<'static, (Subtitles, State, Vec<Command>)> {
        let script = format!(
            "[Script Info]\nPlayResX: 1920\nPlayResY: 1080\n\n[Events]\n\
             Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n\
             Dialogue: 0,0:00:01.00,0:00:03.00,Default,,0,0,0,,{text}\n"
        );
        let subs = subtitles(&script, Format::Ass, 2000);
        Harness::builder()
            .with_size(vec2(1280.0, 720.0))
            .allow_missing_glyphs()
            .build_ui_state(
                |ui, (subs, state, out)| {
                    let scene = Scene {
                        subs,
                        video_size: (1920, 1080),
                        font_size: 36.0,
                        respect_ass: true,
                        obscure: Obscure::None,
                        secondary: None,
                        keyboard: None,
                        marks: HashMap::new(),
                    };
                    draw(ui.ctx(), &scene, state, 0.0, out);
                },
                (subs, State::default(), Vec::new()),
            )
    }

    fn graphemes(h: &Harness<'_, (Subtitles, State, Vec<Command>)>) -> Vec<(usize, Rect)> {
        h.state().1.drawn[0].graphemes.clone()
    }

    // T-SUB-019: a quarter turn stands the text up, and its characters still
    // take clicks
    #[test]
    fn a_turned_cue_is_clicked_where_it_is_drawn() {
        let mut h = ass_harness(r"{\pos(960,540)\an5\frz90}食べました");
        h.run();
        let all = graphemes(&h);
        let (first, last) = (all[0].1.center(), all[all.len() - 1].1.center());
        assert!((first.x - last.x).abs() < 2.0, "{first:?} {last:?}");
        // anticlockwise: it reads upwards
        assert!(first.y > last.y + 50.0, "{first:?} {last:?}");
        let (offset, rect) = all[1];
        click(&mut h, rect.center());
        assert!(
            h.state()
                .2
                .iter()
                .any(|c| matches!(c, Command::GraphemeClicked { offset: o, .. } if *o == offset)),
            "{:?}",
            h.state().2
        );
    }

    // T-SUB-019
    #[test]
    fn a_move_is_halfway_at_half_time() {
        let mut h = ass_harness(r"{\an5\move(100,100,1820,100)}字");
        h.run();
        let x = graphemes(&h)[0].1.center().x;
        assert!((x - 640.0).abs() < 2.0, "{x}");
    }

    // T-SUB-019: scaled around its anchor
    #[test]
    fn a_scaled_cue_stays_on_its_anchor() {
        let mut plain = ass_harness(r"{\pos(960,540)\an5}字字");
        plain.run();
        let mut wide = ass_harness(r"{\pos(960,540)\an5\fscx200}字字");
        wide.run();
        let width = |h: &Harness<'_, (Subtitles, State, Vec<Command>)>| {
            let all = graphemes(h);
            all[1].1.max.x - all[0].1.min.x
        };
        assert!((width(&wide) / width(&plain) - 2.0).abs() < 0.05);
        let all = graphemes(&wide);
        assert!(((all[0].1.min.x + all[1].1.max.x) / 2.0 - 640.0).abs() < 2.0);
    }

    // T-SUB-019
    #[test]
    fn a_clip_is_where_the_cue_shows() {
        let frame = |clip| Frame {
            position: None,
            origin: None,
            rotation: [0.0; 3],
            scale: [1.0; 2],
            opacity: 1.0,
            size: None,
            color: None,
            clip,
        };
        let window = window();
        assert_eq!(clip_regions(None, window, window), [window]);
        let clip = nikomiru_subs::effects::Clip {
            min: (0.25, 0.25),
            max: (0.75, 0.75),
            inverse: false,
        };
        let inner = Rect::from_min_max(pos2(320.0, 180.0), pos2(960.0, 540.0));
        assert_eq!(
            clip_regions(Some(&frame(Some(clip))), window, window),
            [inner]
        );
        let outside = clip_regions(
            Some(&frame(Some(nikomiru_subs::effects::Clip {
                inverse: true,
                ..clip
            }))),
            window,
            window,
        );
        assert_eq!(outside.len(), 4);
        assert!(outside.iter().all(|r| !r.intersects(inner.shrink(1.0))));
    }

    // T-SUB-019: with ASS styles not respected, the text is only text
    #[test]
    fn effects_follow_the_setting() {
        let script = "[Script Info]\nPlayResX: 1920\nPlayResY: 1080\n\n[Events]\n\
             Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n\
             Dialogue: 0,0:00:01.00,0:00:03.00,Default,,0,0,0,,{\\frz90}字\n";
        let subs = subtitles(script, Format::Ass, 2000);
        assert!(subs.shown(false).all(|s| s.frame.is_none()));
        assert!(subs.shown(true).all(|s| s.frame.is_some()));
    }

    fn click(h: &mut Harness<'_, (Subtitles, State, Vec<Command>)>, at: Pos2) {
        h.hover_at(at);
        h.drag_at(at);
        h.drop_at(at);
        h.run();
    }

    // T-SUB-017
    #[test]
    fn the_secondary_subtitle_is_drawn_apart_and_never_clicked() {
        let main = || {
            subtitles(
                "1\n00:00:01,000 --> 00:00:03,000\n食べました\n",
                Format::Srt,
                1500,
            )
        };
        let english = || {
            subtitles(
                "1\n00:00:01,200 --> 00:00:02,800\nI ate.\n",
                Format::Srt,
                1500,
            )
        };
        // at the top of the video
        let mut h = with_secondary(
            main(),
            Obscure::None,
            Some((english(), Obscure::None, false)),
        );
        h.run();
        let drawn = h.state().1.drawn.clone();
        let (sec, prim): (Vec<_>, Vec<_>) = drawn.iter().partition(|d| d.secondary);
        assert_eq!((sec.len(), prim.len()), (1, 1), "{drawn:?}");
        assert!(sec[0].graphemes.is_empty(), "no lookups in a translation");
        assert!(sec[0].rect.max.y < 360.0 && prim[0].rect.min.y > 360.0);
        assert!(sec[0].rect.height() < prim[0].rect.height());
        click(&mut h, sec[0].rect.center());
        assert!(
            !h.state()
                .2
                .iter()
                .any(|c| matches!(c, Command::GraphemeClicked { .. })),
            "{:?}",
            h.state().2
        );
        // above the main one at the bottom, hidden on its own
        let mut h = with_secondary(
            main(),
            Obscure::None,
            Some((english(), Obscure::Hidden, true)),
        );
        h.run();
        let drawn = h.state().1.drawn.clone();
        let sec = drawn.iter().find(|d| d.secondary).unwrap();
        let prim = drawn.iter().find(|d| !d.secondary).unwrap();
        assert!(sec.rect.max.y <= prim.rect.min.y, "{drawn:?}");
        assert!(!sec.painted && prim.painted);
    }

    // T-SUB-009
    #[test]
    fn a_click_on_a_grapheme_sends_its_byte_offset() {
        let subs = subtitles(
            "1\n00:00:01,000 --> 00:00:03,000\n食べました\n",
            Format::Srt,
            1500,
        );
        let mut h = harness(subs, Obscure::None);
        h.run();
        let drawn = h.state().1.drawn[0].clone();
        assert_eq!(drawn.cue, 0);
        let offsets: Vec<usize> = drawn.graphemes.iter().map(|g| g.0).collect();
        assert_eq!(offsets, [0, 3, 6, 9, 12]);
        // centred above the bottom margin of the video
        assert!((drawn.rect.center().x - 640.0).abs() < 1.0, "{drawn:?}");
        assert!((drawn.rect.max.y - (720.0 - 50.0)).abs() < 1.0, "{drawn:?}");
        // the second grapheme: べ, byte 3; the popup goes next to the whole cue
        let (_, be) = drawn.graphemes[1];
        click(&mut h, be.center());
        let out = std::mem::take(&mut h.state_mut().2);
        let clicks: Vec<_> = out
            .iter()
            .filter_map(|c| match c {
                Command::GraphemeClicked {
                    cue,
                    offset,
                    anchor,
                } => Some((*cue, *offset, *anchor)),
                _ => None,
            })
            .collect();
        let r = drawn.rect;
        let anchor = popup::Rect {
            x: f64::from(r.min.x),
            y: f64::from(r.min.y),
            w: f64::from(r.width()),
            h: f64::from(r.height()),
        };
        assert_eq!(clicks, [(0, 3, anchor)]);
        // the click is the grapheme's only: the video does not play or pause
        assert!(
            !out.iter().any(|c| matches!(c, Command::VideoClicked)),
            "{out:?}"
        );
        // outside the text it is the video's
        click(&mut h, pos2(100.0, 100.0));
        let out = std::mem::take(&mut h.state_mut().2);
        assert!(
            out.iter().any(|c| matches!(c, Command::VideoClicked)),
            "{out:?}"
        );
        assert!(
            !out.iter()
                .any(|c| matches!(c, Command::GraphemeClicked { .. }))
        );
    }

    // T-POP-003
    #[test]
    fn hovering_reports_the_grapheme_and_entering_and_leaving_once() {
        let subs = subtitles(
            "1\n00:00:01,000 --> 00:00:03,000\n食べました\n",
            Format::Srt,
            1500,
        );
        let mut h = harness(subs, Obscure::None);
        h.run();
        let (_, ma) = h.state().1.drawn[0].graphemes[2];
        h.event_modifiers(
            egui::Event::PointerMoved(ma.center()),
            egui::Modifiers::SHIFT,
        );
        h.run();
        h.hover_at(ma.center() + vec2(1.0, 0.0));
        h.run();
        let out = std::mem::take(&mut h.state_mut().2);
        let hovers: Vec<_> = out
            .iter()
            .filter_map(|c| match c {
                Command::GraphemeHovered {
                    cue,
                    offset,
                    shift,
                    x,
                    ..
                } => Some((*cue, *offset, *shift, *x)),
                _ => None,
            })
            .collect();
        assert!(
            hovers.contains(&(0, 6, true, f64::from(ma.center().x))),
            "{hovers:?}"
        );
        assert!(
            hovers.contains(&(0, 6, false, f64::from(ma.center().x + 1.0))),
            "{hovers:?}"
        );
        let entered: Vec<_> = out
            .iter()
            .filter(|c| matches!(c, Command::SubtitleHover(_)))
            .collect();
        assert!(
            matches!(entered[..], [Command::SubtitleHover(true)]),
            "{out:?}"
        );
        // leaving the cue: once
        h.hover_at(pos2(100.0, 100.0));
        h.run();
        h.hover_at(pos2(110.0, 100.0));
        h.run();
        let out = std::mem::take(&mut h.state_mut().2);
        let left: Vec<_> = out
            .iter()
            .filter(|c| matches!(c, Command::SubtitleHover(_)))
            .collect();
        assert!(
            matches!(left[..], [Command::SubtitleHover(false)]),
            "{out:?}"
        );
        assert!(
            !out.iter()
                .any(|c| matches!(c, Command::GraphemeHovered { .. }))
        );
    }

    // T-SUB-014
    #[test]
    fn hidden_cues_are_not_drawn_but_still_answer_the_pointer() {
        let subs = subtitles(
            "1\n00:00:01,000 --> 00:00:03,000\n食べました\n",
            Format::Srt,
            1500,
        );
        let mut h = harness(subs, Obscure::Hidden);
        h.run();
        let drawn = h.state().1.drawn[0].clone();
        assert!(!drawn.painted);
        let text_shapes = h
            .output()
            .shapes
            .iter()
            .filter(|s| matches!(s.shape, egui::Shape::Text(_)))
            .count();
        assert_eq!(text_shapes, 0);
        click(&mut h, drawn.graphemes[0].1.center());
        let out = std::mem::take(&mut h.state_mut().2);
        assert!(
            out.contains_command(|c| matches!(c, Command::GraphemeClicked { offset: 0, .. })),
            "{out:?}"
        );
        assert!(
            out.contains_command(|c| matches!(c, Command::SubtitleHover(true))),
            "{out:?}"
        );
    }

    // T-POP-002
    #[test]
    fn the_highlight_covers_the_matched_graphemes_and_v_draws_nothing() {
        // a flag and a combining mark are one grapheme each
        let mut subs = subtitles(
            "1\n00:00:01,000 --> 00:00:03,000\n🇯🇵か\u{3099}食べる\n",
            Format::Srt,
            1500,
        );
        let cue = subs.cue(0).unwrap();
        let offsets: Vec<usize> = cue.graphemes().iter().map(|g| g.offset).collect();
        // 🇯🇵 (8 bytes), が (6), 食, べ, る
        assert_eq!(offsets, [0, 8, 14, 17, 20]);
        subs.highlight = Some((0, 8..20));
        let mut h = harness(subs, Obscure::None);
        h.run();
        assert_eq!(h.state().1.drawn[0].highlighted, 3, "が, 食, べ");
        // the clicked grapheme alone while the lookup runs
        h.state_mut().0.highlight = None;
        h.state_mut().0.selection = Some((0, 14));
        h.run();
        assert_eq!(h.state().1.drawn[0].highlighted, 1);
        // V: nothing at all, not even a target
        h.state_mut().0.disabled = true;
        h.run();
        assert!(h.state().1.drawn.is_empty());
    }

    // T-LKP-051, T-LKP-061
    #[test]
    fn readings_and_mined_words_are_drawn_over_their_graphemes() {
        let srt = "1\n00:00:01,000 --> 00:00:03,000\n猫が食べた\n";
        let marks = Marks {
            readings: vec![(0..3, "ねこ".into()), (6..9, "た".into())],
            mined: std::iter::once(6..15).collect(),
        };
        let mut h = build(
            subtitles(srt, Format::Srt, 1500),
            Obscure::None,
            None,
            HashMap::from([(0, marks.clone())]),
        );
        h.run();
        let marked = h.state().1.drawn[0].clone();
        assert_eq!((marked.readings, marked.mined), (2, 1));
        let mut plain = harness(subtitles(srt, Format::Srt, 1500), Obscure::None);
        plain.run();
        let plain = plain.state().1.drawn[0].clone();
        assert_eq!((plain.readings, plain.mined), (0, 0));
        // the readings take room above the line; the text stays clickable
        assert!(marked.rect.height() > plain.rect.height());
        assert_eq!(marked.graphemes.len(), 5);
        // the file's own ruby wins
        let vtt = "WEBVTT\n\n00:00:01.000 --> 00:00:03.000\n<ruby>猫<rt>ネコ</rt></ruby>が食べた\n";
        let mut h = build(
            subtitles(vtt, Format::Vtt, 1500),
            Obscure::None,
            None,
            HashMap::from([(0, marks)]),
        );
        h.run();
        assert_eq!(h.state().1.drawn[0].readings, 1);
        assert_eq!(h.state().1.drawn[0].mined, 1);
    }

    trait Contains {
        fn contains_command(&self, f: impl Fn(&Command) -> bool) -> bool;
    }

    impl Contains for Vec<Command> {
        fn contains_command(&self, f: impl Fn(&Command) -> bool) -> bool {
            self.iter().any(f)
        }
    }
}
