// SPDX-License-Identifier: GPL-3.0-only

//! The colors of the interface (SET-003): one set of tokens for the dark
//! theme, one for the light, with the accent chosen in the settings. The
//! pages and the dictionary popups take them; what is drawn on the video
//! (the bar, the subtitles, their list, the toasts) stays dark, whatever the
//! theme, as video players do.
//!
//! Every text has a contrast of at least 4.5:1 with what it is drawn on
//! (WCAG AA), over a black or a white video alike: the test below computes
//! it from the tokens.

use std::cell::Cell;

use egui::Color32;

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

/// The default accent.
pub(crate) const ACCENT: Color32 = rgb(0x6a, 0xa0, 0xff);

const DARK_POPUP: PopupColors = PopupColors {
    panel: rgba(0x1b, 0x1d, 0x22, 0xf2),
    panel_border: rgba(0xff, 0xff, 0xff, 0x40),
    text: rgb(0xe8, 0xe8, 0xe8),
    headword: Color32::WHITE,
    weak: rgb(0x9a, 0xa0, 0xaa),
    link: rgb(0x8a, 0xb4, 0xff),
    marker: rgb(0xaa, 0xb2, 0xbf),
    reading: rgb(0xc5, 0xcf, 0xf5),
    cursor: ACCENT,
    section: rgb(0x26, 0x2a, 0x31),
    section_text: rgb(0xc9, 0xd1, 0xdd),
    trace: rgb(0x24, 0x46, 0x5a),
    frequency: rgb(0x2f, 0x3a, 0x2a),
    onyomi: rgb(0x3a, 0x2a, 0x44),
    kunyomi: rgb(0x2a, 0x3a, 0x44),
    chip: rgb(0x3a, 0x3d, 0x44),
    tag_popular: rgb(0x5a, 0x3b, 0x1f),
    tag_part_of_speech: rgb(0x2a, 0x3f, 0x5f),
    tag_frequent: rgb(0x3d, 0x5a, 0x2a),
    tag_archaism: rgb(0x4a, 0x3a, 0x5a),
    pitch: rgb(0xf0, 0xb3, 0x5a),
    cell_border: rgb(0x55, 0x5b, 0x66),
    header_cell: rgb(0x2c, 0x30, 0x38),
    border: rgba(0x90, 0x9a, 0xa8, 0x80),
    grip: rgba(0xff, 0xff, 0xff, 0x55),
    selection: rgba(0x4a, 0x90, 0xe2, 0x66),
};

const LIGHT_POPUP: PopupColors = PopupColors {
    panel: rgba(0xf9, 0xfa, 0xfb, 0xf5),
    panel_border: rgba(0, 0, 0, 0x30),
    text: rgb(0x1d, 0x1f, 0x24),
    headword: rgb(0x11, 0x13, 0x18),
    weak: rgb(0x55, 0x5c, 0x6a),
    link: rgb(0x1d, 0x5b, 0xbf),
    marker: rgb(0x55, 0x5c, 0x6a),
    reading: rgb(0x33, 0x42, 0x8a),
    cursor: ACCENT,
    section: rgb(0xe7, 0xea, 0xef),
    section_text: rgb(0x34, 0x3b, 0x47),
    trace: rgb(0xcf, 0xe3, 0xee),
    frequency: rgb(0xdc, 0xea, 0xd2),
    onyomi: rgb(0xec, 0xe0, 0xf4),
    kunyomi: rgb(0xd9, 0xe8, 0xf2),
    chip: rgb(0xe2, 0xe5, 0xeb),
    tag_popular: rgb(0xf6, 0xe0, 0xc8),
    tag_part_of_speech: rgb(0xd6, 0xe2, 0xf4),
    tag_frequent: rgb(0xdc, 0xeb, 0xd2),
    tag_archaism: rgb(0xe7, 0xdc, 0xf2),
    pitch: rgb(0xb0, 0x5a, 0x00),
    cell_border: rgb(0xb2, 0xb8, 0xc2),
    header_cell: rgb(0xe8, 0xeb, 0xf0),
    border: rgba(0x6f, 0x79, 0x88, 0x80),
    grip: rgba(0, 0, 0, 0x40),
    selection: rgba(0x4a, 0x90, 0xe2, 0x55),
};

impl Palette {
    /// The dark theme with the default accent.
    pub(crate) const DARK: Palette = Palette {
        dark: true,
        backdrop: rgba(0, 0, 0, 0xa6),
        page: rgb(0x10, 0x12, 0x16),
        card: rgba(0x20, 0x22, 0x27, 0xf0),
        card_border: rgba(0xff, 0xff, 0xff, 0x40),
        dialog: rgb(0x2a, 0x2d, 0x34),
        row: rgba(0xff, 0xff, 0xff, 0x0c),
        row_hover: rgba(0xff, 0xff, 0xff, 0x1c),
        row_off: rgba(0, 0, 0, 0x10),
        placeholder: rgb(0x26, 0x2d, 0x3d),
        track: rgba(0xff, 0xff, 0xff, 0x30),
        text: Color32::WHITE,
        details: rgb(0x9a, 0xa0, 0xaa),
        off: rgb(0x8a, 0x8f, 0x99),
        good: rgb(0x7c, 0xd3, 0x8a),
        bad: rgb(0xf0, 0xb3, 0x5a),
        accent: ACCENT,
        accent_text: ACCENT,
        selection: rgb(0x3a, 0x52, 0x80),
        popup: DARK_POPUP,
    };

    /// The light theme with the default accent.
    pub(crate) const LIGHT: Palette = Palette {
        dark: false,
        backdrop: rgba(0, 0, 0, 0x80),
        page: rgb(0xee, 0xf0, 0xf3),
        card: rgba(0xf6, 0xf7, 0xf9, 0xf8),
        card_border: rgba(0, 0, 0, 0x30),
        dialog: rgb(0xfb, 0xfc, 0xfd),
        row: rgba(0, 0, 0, 0x0a),
        row_hover: rgba(0, 0, 0, 0x18),
        row_off: rgba(0, 0, 0, 0x03),
        placeholder: rgb(0xd9, 0xde, 0xe6),
        track: rgba(0, 0, 0, 0x26),
        text: rgb(0x1d, 0x1f, 0x24),
        details: rgb(0x52, 0x59, 0x66),
        off: rgb(0x5c, 0x63, 0x70),
        good: rgb(0x1d, 0x6e, 0x30),
        bad: rgb(0x96, 0x4b, 0x00),
        accent: ACCENT,
        accent_text: rgb(0x1d, 0x5b, 0xbf),
        selection: rgb(0xd2, 0xe2, 0xff),
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
    ctx.style_mut_of(theme, |style| {
        let v = &mut style.visuals;
        v.selection.bg_fill = palette.selection;
        v.selection.stroke.color = palette.text;
        v.hyperlink_color = palette.accent_text;
        v.text_cursor.stroke.color = palette.accent_text;
    });
}

/// The style of what is drawn on the video: dark, whatever the theme.
pub(crate) fn on_video(ui: &mut egui::Ui) {
    if !ui.visuals().dark_mode {
        ui.set_style(ui.ctx().style_of(egui::Theme::Dark));
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
