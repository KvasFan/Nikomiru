// SPDX-License-Identifier: GPL-3.0-only

//! The colors of the interface (SET-003): one set of tokens for the dark
//! theme, one for the light, with the accent chosen in the settings. Their
//! look follows GNOME's Adwaita (SET-009): graphite surfaces stepping up from
//! the page to the card and the dialog, flat buttons on a translucent fill,
//! rounded corners, GNOME's blue as the accent. The
//! pages and the dictionary popups take them; what is drawn on the video
//! (the bar, the subtitles, their list, the toasts) stays dark, whatever the
//! theme, as video players do.
//!
//! Every text has a contrast of at least 4.5:1 with what it is drawn on
//! (WCAG AA), over a black or a white video alike: the test below computes
//! it from the tokens.

use std::cell::Cell;

use egui::Color32;

use crate::words::{Category, Underline};

/// The colors of the pages and of the popups.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Palette {
    /// The dark theme's.
    pub dark: bool,
    /// Shades the video under a page.
    pub backdrop: Color32,
    /// A page that covers the whole window (the library).
    pub page: Color32,
    /// A page's card.
    pub card: Color32,
    /// Its border.
    pub card_border: Color32,
    /// A dialog over a page.
    pub dialog: Color32,
    /// A row or a box inside a card.
    pub row: Color32,
    /// A row under the pointer.
    pub row_hover: Color32,
    /// A row turned off.
    pub row_off: Color32,
    /// What stands for a picture not there yet, an empty day.
    pub placeholder: Color32,
    /// The track of a progress bar.
    pub track: Color32,
    /// Text.
    pub text: Color32,
    /// Hints and details.
    pub details: Color32,
    /// The text of what is turned off.
    pub off: Color32,
    /// Good news.
    pub good: Color32,
    /// Bad news, a warning.
    pub bad: Color32,
    /// Bars, drop lines, what is chosen.
    pub accent: Color32,
    /// The accent as text: readable on the card.
    pub accent_text: Color32,
    /// What is selected: a tab, a list item.
    pub selection: Color32,
    /// The dictionary popups.
    pub popup: PopupColors,
}

/// The colors of the dictionary popups.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PopupColors {
    pub panel: Color32,
    pub panel_border: Color32,
    pub text: Color32,
    /// The headword and the kanji.
    pub headword: Color32,
    /// Details: dictionary names, counts.
    pub weak: Color32,
    pub link: Color32,
    /// List markers.
    pub marker: Color32,
    pub reading: Color32,
    /// The keyboard's place.
    pub cursor: Color32,
    /// A section's header, and its text.
    pub section: Color32,
    pub section_text: Color32,
    /// The chips of the deinflection, the frequencies, the readings.
    pub trace: Color32,
    pub frequency: Color32,
    pub onyomi: Color32,
    pub kunyomi: Color32,
    pub chip: Color32,
    /// The tags by category.
    pub tag_popular: Color32,
    pub tag_part_of_speech: Color32,
    pub tag_frequent: Color32,
    pub tag_archaism: Color32,
    /// The pitch accent's line.
    pub pitch: Color32,
    /// Tables.
    pub cell_border: Color32,
    pub header_cell: Color32,
    /// The border of the content's boxes, the resize grip.
    pub border: Color32,
    pub grip: Color32,
    /// Selected text.
    pub selection: Color32,
}

const fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied_const(r, g, b, a)
}

/// The default accent: GNOME's blue.
pub(crate) const ACCENT: Color32 = rgb(0x35, 0x84, 0xe4);

const DARK_POPUP: PopupColors = PopupColors {
    panel: rgba(0x2e, 0x2e, 0x32, 0xf8),
    panel_border: rgba(0xff, 0xff, 0xff, 0x24),
    text: rgb(0xf2, 0xf2, 0xf3),
    headword: Color32::WHITE,
    weak: rgb(0xa8, 0xa8, 0xae),
    link: rgb(0x78, 0xae, 0xed),
    marker: rgb(0xb0, 0xb0, 0xb6),
    reading: rgb(0x99, 0xc1, 0xf1),
    cursor: ACCENT,
    section: rgb(0x38, 0x38, 0x3d),
    section_text: rgb(0xde, 0xdd, 0xda),
    trace: rgb(0x1f, 0x3d, 0x5c),
    frequency: rgb(0x23, 0x3f, 0x2c),
    onyomi: rgb(0x40, 0x2c, 0x4a),
    kunyomi: rgb(0x22, 0x3a, 0x4a),
    chip: rgb(0x45, 0x45, 0x4a),
    tag_popular: rgb(0x5c, 0x3a, 0x14),
    tag_part_of_speech: rgb(0x1f, 0x3d, 0x5c),
    tag_frequent: rgb(0x23, 0x4a, 0x2c),
    tag_archaism: rgb(0x4a, 0x30, 0x5a),
    pitch: rgb(0xff, 0xbe, 0x6f),
    cell_border: rgb(0x55, 0x55, 0x5b),
    header_cell: rgb(0x3a, 0x3a, 0x40),
    border: rgba(0xff, 0xff, 0xff, 0x30),
    grip: rgba(0xff, 0xff, 0xff, 0x50),
    selection: rgba(0x35, 0x84, 0xe4, 0x70),
};

const LIGHT_POPUP: PopupColors = PopupColors {
    panel: rgba(0xff, 0xff, 0xff, 0xfa),
    panel_border: rgba(0, 0, 6, 0x24),
    text: rgb(0x24, 0x24, 0x28),
    headword: rgb(0x14, 0x14, 0x18),
    weak: rgb(0x5e, 0x5c, 0x64),
    link: rgb(0x1a, 0x5f, 0xb4),
    marker: rgb(0x5e, 0x5c, 0x64),
    reading: rgb(0x1c, 0x47, 0x8a),
    cursor: ACCENT,
    section: rgb(0xeb, 0xeb, 0xed),
    section_text: rgb(0x3d, 0x38, 0x46),
    trace: rgb(0xd5, 0xe6, 0xfa),
    frequency: rgb(0xd8, 0xf0, 0xdd),
    onyomi: rgb(0xef, 0xe0, 0xf6),
    kunyomi: rgb(0xd9, 0xea, 0xf5),
    chip: rgb(0xe6, 0xe6, 0xe9),
    tag_popular: rgb(0xfb, 0xe3, 0xc8),
    tag_part_of_speech: rgb(0xd5, 0xe6, 0xfa),
    tag_frequent: rgb(0xd8, 0xf0, 0xdd),
    tag_archaism: rgb(0xec, 0xdf, 0xf5),
    pitch: rgb(0xb3, 0x52, 0x00),
    cell_border: rgb(0xc0, 0xbf, 0xc6),
    header_cell: rgb(0xee, 0xee, 0xf0),
    border: rgba(0, 0, 6, 0x30),
    grip: rgba(0, 0, 6, 0x40),
    selection: rgba(0x35, 0x84, 0xe4, 0x50),
};

impl Palette {
    /// The dark theme with the default accent.
    pub(crate) const DARK: Palette = Palette {
        dark: true,
        backdrop: rgba(0, 0, 0, 0xb0),
        page: rgb(0x22, 0x22, 0x26),
        card: rgba(0x2e, 0x2e, 0x32, 0xf6),
        card_border: rgba(0xff, 0xff, 0xff, 0x1a),
        dialog: rgb(0x36, 0x36, 0x3a),
        row: rgba(0xff, 0xff, 0xff, 0x10),
        row_hover: rgba(0xff, 0xff, 0xff, 0x20),
        row_off: rgba(0, 0, 0, 0x18),
        placeholder: rgb(0x3a, 0x3a, 0x3f),
        track: rgba(0xff, 0xff, 0xff, 0x26),
        text: Color32::WHITE,
        details: rgb(0xa8, 0xa8, 0xae),
        off: rgb(0x96, 0x96, 0x9c),
        good: rgb(0x8f, 0xf0, 0xa4),
        bad: rgb(0xff, 0xa3, 0x48),
        accent: ACCENT,
        accent_text: rgb(0x78, 0xae, 0xed),
        selection: rgb(0x1c, 0x47, 0x7a),
        popup: DARK_POPUP,
    };

    /// The light theme with the default accent.
    pub(crate) const LIGHT: Palette = Palette {
        dark: false,
        backdrop: rgba(0, 0, 0, 0x80),
        page: rgb(0xfa, 0xfa, 0xfb),
        card: rgba(0xff, 0xff, 0xff, 0xfa),
        card_border: rgba(0, 0, 6, 0x1e),
        dialog: rgb(0xff, 0xff, 0xff),
        row: rgba(0, 0, 6, 0x0c),
        row_hover: rgba(0, 0, 6, 0x18),
        row_off: rgba(0, 0, 6, 0x04),
        placeholder: rgb(0xde, 0xdd, 0xe2),
        track: rgba(0, 0, 6, 0x22),
        text: rgb(0x24, 0x24, 0x28),
        details: rgb(0x5e, 0x5c, 0x64),
        off: rgb(0x68, 0x66, 0x6e),
        good: rgb(0x1b, 0x6c, 0x3a),
        bad: rgb(0x9c, 0x46, 0x00),
        accent: ACCENT,
        accent_text: rgb(0x1a, 0x5f, 0xb4),
        selection: rgb(0xd5, 0xe6, 0xfa),
        popup: LIGHT_POPUP,
    };

    /// The theme `dark` or light, with `accent`.
    pub(crate) fn new(dark: bool, accent: Color32) -> Palette {
        let base = if dark { Self::DARK } else { Self::LIGHT };
        let card = worst_background(base.card, base.backdrop, dark);
        // the library's cards on its page
        let tile = over(base.row, base.page);
        let surface = if dark { Color32::BLACK } else { Color32::WHITE };
        Palette {
            accent,
            accent_text: readable(accent, &[card, tile], dark),
            // the accent, faint enough for the text to read on it
            selection: mix(accent, surface, if dark { 0.45 } else { 0.25 }),
            popup: PopupColors {
                cursor: accent,
                ..base.popup
            },
            ..base
        }
    }
}

/// `top` over the opaque `bottom`.
pub(crate) fn over(top: Color32, bottom: Color32) -> Color32 {
    let a = f32::from(top.a()) / 255.0;
    // Color32 is premultiplied: the top's channels are already scaled by its alpha
    let c = |t: u8, b: u8| (f32::from(t) + f32::from(b) * (1.0 - a)).round().min(255.0) as u8;
    Color32::from_rgb(
        c(top.r(), bottom.r()),
        c(top.g(), bottom.g()),
        c(top.b(), bottom.b()),
    )
}

/// `a` with a share `t` of itself and `1 - t` of `b`, both opaque.
fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let c = |x: u8, y: u8| (f32::from(x) * t + f32::from(y) * (1.0 - t)).round() as u8;
    Color32::from_rgb(c(a.r(), b.r()), c(a.g(), b.g()), c(a.b(), b.b()))
}

/// The relative luminance of an opaque color (WCAG 2).
fn luminance(c: Color32) -> f32 {
    let channel = |v: u8| {
        let v = f32::from(v) / 255.0;
        if v <= 0.040_45 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(c.r()) + 0.7152 * channel(c.g()) + 0.0722 * channel(c.b())
}

/// The contrast ratio of two opaque colors, 1 to 21.
pub(crate) fn contrast(a: Color32, b: Color32) -> f32 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// The card over the backdrop over the video that makes text hardest to
/// read: a white video under a dark theme, a black one under a light.
fn worst_background(card: Color32, backdrop: Color32, dark: bool) -> Color32 {
    let video = if dark { Color32::WHITE } else { Color32::BLACK };
    over(card, over(backdrop, video))
}

/// `accent` made lighter (dark theme) or darker (light) until it reads on
/// every one of `backgrounds` at 4.5:1.
fn readable(accent: Color32, backgrounds: &[Color32], dark: bool) -> Color32 {
    let toward = if dark { Color32::WHITE } else { Color32::BLACK };
    let mut color = Color32::from_rgb(accent.r(), accent.g(), accent.b());
    let mut t = 1.0;
    while backgrounds.iter().any(|b| contrast(color, *b) < 4.5) && t > 0.0 {
        t -= 0.05;
        color = mix(accent, toward, t);
    }
    color
}

thread_local! {
    static CURRENT: Cell<Palette> = const { Cell::new(Palette::DARK) };
}

/// The palette in use (the interface runs on one thread).
pub(crate) fn current() -> Palette {
    CURRENT.with(Cell::get)
}

/// Puts the theme `dark` or light with `accent` in use: the palette, and
/// egui's own widgets.
pub(crate) fn apply(ctx: &egui::Context, dark: bool, accent: Color32) {
    let palette = Palette::new(dark, accent);
    CURRENT.with(|c| c.set(palette));
    let theme = if dark {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    };
    ctx.set_theme(theme);
    ctx.style_mut_of(theme, |style| adwaita(style, &palette));
}

/// egui's widgets in the manner of Adwaita (SET-009): flat buttons on a
/// translucent fill that brightens under the pointer, no borders but a
/// faint one on text fields, rounded corners, soft shadows.
fn adwaita(style: &mut egui::Style, p: &Palette) {
    use egui::{CornerRadius, Shadow, Stroke};
    let (ink, alpha) = if p.dark { (0xff, 1.0) } else { (6, 0.8) };
    let fill = |a: u8| {
        let a = (f32::from(a) * alpha).round() as u8;
        if p.dark {
            rgba(ink, ink, ink, a)
        } else {
            rgba(0, 0, ink, a)
        }
    };
    let v = &mut style.visuals;
    v.panel_fill = p.page;
    v.window_fill = p.dialog;
    v.window_stroke = Stroke::new(1.0, p.card_border);
    v.window_corner_radius = CornerRadius::same(12);
    v.menu_corner_radius = CornerRadius::same(10);
    v.window_shadow = Shadow {
        offset: [0, 4],
        blur: 18,
        spread: 0,
        color: rgba(0, 0, 0, if p.dark { 0x70 } else { 0x30 }),
    };
    v.popup_shadow = Shadow {
        offset: [0, 2],
        blur: 10,
        spread: 0,
        color: rgba(0, 0, 0, if p.dark { 0x60 } else { 0x28 }),
    };
    // text fields and the inside of sliders: a step up from the page
    v.extreme_bg_color = over(fill(0x14), p.page);
    v.text_edit_bg_color = Some(over(fill(0x14), p.page));
    v.faint_bg_color = over(fill(0x08), p.page);
    v.code_bg_color = over(fill(0x14), p.page);
    v.selection.bg_fill = p.selection;
    v.selection.stroke = Stroke::new(1.0, p.text);
    v.hyperlink_color = p.accent_text;
    v.text_cursor.stroke = Stroke::new(2.0, p.accent_text);
    v.warn_fg_color = p.bad;
    v.error_fg_color = p.bad;
    v.window_highlight_topmost = false;
    let radius = CornerRadius::same(6);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p.page;
    w.noninteractive.weak_bg_fill = p.page;
    w.noninteractive.bg_stroke = Stroke::new(1.0, fill(0x18));
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
    w.noninteractive.corner_radius = radius;
    for (state, a) in [
        (&mut w.inactive, 0x1a),
        (&mut w.hovered, 0x26),
        (&mut w.active, 0x40),
        (&mut w.open, 0x26),
    ] {
        state.weak_bg_fill = fill(a);
        state.bg_fill = fill(a);
        state.bg_stroke = Stroke::NONE;
        state.fg_stroke = Stroke::new(1.0, p.text);
        state.corner_radius = radius;
        state.expansion = 0.0;
    }
    // a text field's outline, and the focus in the accent
    w.inactive.bg_stroke = Stroke::new(1.0, fill(0x10));
    w.hovered.bg_stroke = Stroke::new(1.0, fill(0x30));
    w.active.bg_stroke = Stroke::new(2.0, p.accent);
    // checkboxes and sliders: the accent when on
    w.active.bg_fill = p.accent;
    let s = &mut style.spacing;
    s.button_padding = egui::vec2(10.0, 5.0);
    s.item_spacing = egui::vec2(8.0, 6.0);
    s.menu_margin = egui::Margin::same(8);
    s.window_margin = egui::Margin::same(12);
}

/// The fill and the text of a page's main action, from `accent` (SET-009):
/// white on the accent, the accent darkened until white reads on it at
/// 4.5:1, or black on a light accent that black reads on.
pub(crate) fn suggested_colors(accent: Color32) -> (Color32, Color32) {
    let accent = Color32::from_rgb(accent.r(), accent.g(), accent.b());
    if contrast(Color32::WHITE, accent) >= 4.5 {
        return (accent, Color32::WHITE);
    }
    if contrast(Color32::BLACK, accent) >= 7.0 {
        return (accent, Color32::BLACK);
    }
    let mut t = 1.0;
    let mut fill = accent;
    while contrast(Color32::WHITE, fill) < 4.5 && t > 0.0 {
        t -= 0.05;
        fill = mix(accent, Color32::BLACK, t);
    }
    (fill, Color32::WHITE)
}

/// A page's main action, filled with the accent: Adwaita's suggested action
/// (SET-009).
pub(crate) fn suggested(text: impl Into<String>) -> egui::Button<'static> {
    let (fill, ink) = suggested_colors(current().accent);
    egui::Button::new(egui::RichText::new(text.into()).color(ink)).fill(fill)
}

/// The style of what is drawn on the video: dark, whatever the theme.
pub(crate) fn on_video(ui: &mut egui::Ui) {
    if !ui.visuals().dark_mode {
        ui.set_style(ui.ctx().style_of(egui::Theme::Dark));
    }
}

/// The line under a word of the subtitles, by why it is there, drawn over
/// the video in either theme (LKP-061, KNW-002).
pub(crate) fn underline(why: Underline) -> Color32 {
    match why {
        Underline::Mined => Color32::from_rgb(0x6f, 0xcf, 0x97),
        Underline::Unknown => Color32::from_rgb(0x62, 0xa0, 0xea),
        Underline::Learning => Color32::from_rgb(0xf6, 0xd3, 0x2d),
        // the usual colours of the accents (LKP-062)
        Underline::Pitch(p) => match p {
            Category::Heiban => Color32::from_rgb(0x4a, 0x9e, 0xff),
            Category::Atamadaka => Color32::from_rgb(0xff, 0x5c, 0x5c),
            Category::Nakadaka => Color32::from_rgb(0xff, 0xa3, 0x48),
            Category::Odaka => Color32::from_rgb(0x57, 0xd9, 0x6b),
            Category::Kifuku => Color32::from_rgb(0xc0, 0x7c, 0xff),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The worst contrast of `text` on `on`, `on` laid on `base` over a
    /// black and over a white video.
    fn worst(text: Color32, layers: &[Color32]) -> f32 {
        [Color32::BLACK, Color32::WHITE]
            .into_iter()
            .map(|video| {
                let bg = layers
                    .iter()
                    .fold(video, |under, layer| over(*layer, under));
                contrast(text, bg)
            })
            .fold(f32::INFINITY, f32::min)
    }

    // T-SET-003
    #[test]
    fn every_text_reads_on_what_it_is_drawn_on() {
        let accents = [
            ACCENT,
            rgb(0xff, 0xff, 0x00),
            rgb(0x00, 0x00, 0x80),
            rgb(0xe0, 0x40, 0x40),
            rgb(0x80, 0x80, 0x80),
        ];
        for dark in [true, false] {
            for accent in accents {
                let p = Palette::new(dark, accent);
                let card = [p.backdrop, p.card];
                let row = [p.backdrop, p.card, p.row];
                let row_off = [p.backdrop, p.card, p.row_off];
                let pairs: Vec<(&str, Color32, Vec<Color32>)> = vec![
                    ("text/card", p.text, card.to_vec()),
                    ("details/card", p.details, card.to_vec()),
                    ("good/card", p.good, card.to_vec()),
                    ("bad/card", p.bad, card.to_vec()),
                    ("accent/card", p.accent_text, card.to_vec()),
                    ("text/row", p.text, row.to_vec()),
                    ("details/row", p.details, row.to_vec()),
                    ("off/row", p.off, row_off.to_vec()),
                    ("text/selection", p.text, vec![p.selection]),
                    ("text/dialog", p.text, vec![p.dialog]),
                    ("text/page", p.text, vec![p.page]),
                    ("details/page", p.details, vec![p.page, p.row_hover]),
                    ("accent/page", p.accent_text, vec![p.page, p.row]),
                ];
                let q = p.popup;
                let panel = |chip: Color32| vec![q.panel, chip];
                let popup: Vec<(&str, Color32, Vec<Color32>)> = vec![
                    ("text/panel", q.text, vec![q.panel]),
                    ("headword/panel", q.headword, vec![q.panel]),
                    ("weak/panel", q.weak, vec![q.panel]),
                    ("link/panel", q.link, vec![q.panel]),
                    ("marker/panel", q.marker, vec![q.panel]),
                    ("reading/panel", q.reading, vec![q.panel]),
                    ("section", q.section_text, panel(q.section)),
                    ("trace", q.text, panel(q.trace)),
                    ("frequency", q.text, panel(q.frequency)),
                    ("onyomi", q.text, panel(q.onyomi)),
                    ("kunyomi", q.text, panel(q.kunyomi)),
                    ("chip", q.text, panel(q.chip)),
                    ("popular", q.text, panel(q.tag_popular)),
                    ("part of speech", q.text, panel(q.tag_part_of_speech)),
                    ("frequent", q.text, panel(q.tag_frequent)),
                    ("archaism", q.text, panel(q.tag_archaism)),
                    ("header cell", q.text, panel(q.header_cell)),
                    ("selected text", q.text, panel(q.selection)),
                ];
                for (name, text, layers) in pairs.into_iter().chain(popup) {
                    let ratio = worst(text, &layers);
                    assert!(
                        ratio >= 4.5,
                        "{name}, dark {dark}, accent {accent:?}: {ratio:.2}"
                    );
                }
            }
        }
    }

    // T-SET-009: the main action reads on its accent, whatever the accent
    #[test]
    fn the_main_action_reads_on_its_fill() {
        for accent in [
            ACCENT,
            rgb(0xff, 0xff, 0x00),
            rgb(0x00, 0x00, 0x80),
            rgb(0xe0, 0x40, 0x40),
            rgb(0x80, 0x80, 0x80),
            rgb(0x8f, 0xf0, 0xa4),
        ] {
            let (fill, ink) = suggested_colors(accent);
            assert!(contrast(fill, ink) >= 4.5, "{accent:?}: {fill:?} {ink:?}");
        }
        // GNOME's blue is darkened a little for white text
        assert_ne!(suggested_colors(ACCENT).0, ACCENT);
    }

    #[test]
    fn colors_compose_and_compare() {
        assert_eq!(over(Color32::WHITE, Color32::BLACK), Color32::WHITE);
        assert_eq!(
            over(rgba(0, 0, 0, 0x80), Color32::WHITE),
            rgb(0x7f, 0x7f, 0x7f)
        );
        assert!((contrast(Color32::BLACK, Color32::WHITE) - 21.0).abs() < 0.01);
        // a dark accent on the dark theme is lightened, on the light kept
        let navy = rgb(0x00, 0x00, 0x80);
        assert_ne!(Palette::new(true, navy).accent_text, navy);
        assert_eq!(Palette::new(false, navy).accent_text, navy);
    }
}
