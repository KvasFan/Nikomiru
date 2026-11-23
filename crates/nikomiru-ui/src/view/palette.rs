// SPDX-License-Identifier: GPL-3.0-only

//! The colors of the interface (SET-003): one set of tokens for the dark
//! theme, one for the light, with the accent chosen in the settings. Their
//! look follows Fushi 3's Material 3 Expressive (SET-010, D-129): neutral
//! surfaces tinted by the accent, stepping up from the page to the rail, the
//! card and the dialog; pill-shaped buttons, fields and tabs; a light filled
//! main action with dark text in the dark theme; what is chosen on a tonal
//! fill of the accent; large rounded corners. The
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
    /// The navigation rail and the settings' column of tabs: a step up from
    /// the page (SET-010).
    pub rail: Color32,
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
    /// What is selected: a tab, a list item. The accent's tonal container
    /// (SET-010); `text` reads on it.
    pub selection: Color32,
    /// The fill of a page's main action and its text (SET-010).
    pub primary: Color32,
    pub on_primary: Color32,
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
    /// The dark theme before the accent tints it (SET-010): Material 3's
    /// neutral tones, the page at tone 6 up to the dialog at tone 17.
    pub(crate) const DARK: Palette = Palette {
        dark: true,
        backdrop: rgba(0, 0, 0, 0xb0),
        page: rgb(0x12, 0x13, 0x16),
        rail: rgb(0x1a, 0x1b, 0x1f),
        card: rgba(0x1e, 0x1f, 0x23, 0xf6),
        card_border: rgba(0xff, 0xff, 0xff, 0x14),
        dialog: rgb(0x27, 0x28, 0x2d),
        row: rgba(0xff, 0xff, 0xff, 0x0e),
        row_hover: rgba(0xff, 0xff, 0xff, 0x1c),
        row_off: rgba(0, 0, 0, 0x18),
        placeholder: rgb(0x2c, 0x2d, 0x32),
        track: rgba(0xff, 0xff, 0xff, 0x26),
        text: rgb(0xe4, 0xe3, 0xe8),
        details: rgb(0xc2, 0xc4, 0xce),
        off: rgb(0x98, 0x9a, 0xa3),
        good: rgb(0x8f, 0xf0, 0xa4),
        bad: rgb(0xff, 0xb4, 0xab),
        accent: ACCENT,
        accent_text: rgb(0x78, 0xae, 0xed),
        selection: rgb(0x1d, 0x34, 0x52),
        primary: rgb(0x90, 0xbb, 0xf0),
        on_primary: rgb(0x0b, 0x1a, 0x2e),
        popup: DARK_POPUP,
    };

    /// The light theme before the accent tints it (SET-010): the page at
    /// tone 98, the cards a step darker.
    pub(crate) const LIGHT: Palette = Palette {
        dark: false,
        backdrop: rgba(0, 0, 0, 0x80),
        page: rgb(0xf9, 0xf9, 0xfc),
        rail: rgb(0xef, 0xef, 0xf4),
        card: rgba(0xee, 0xee, 0xf3, 0xfa),
        card_border: rgba(0, 0, 6, 0x1a),
        dialog: rgb(0xf3, 0xf3, 0xf8),
        row: rgba(0, 0, 6, 0x0a),
        row_hover: rgba(0, 0, 6, 0x16),
        row_off: rgba(0, 0, 6, 0x04),
        placeholder: rgb(0xdc, 0xdc, 0xe3),
        track: rgba(0, 0, 6, 0x22),
        text: rgb(0x1a, 0x1b, 0x20),
        details: rgb(0x44, 0x47, 0x4f),
        off: rgb(0x55, 0x57, 0x5f),
        good: rgb(0x1b, 0x6c, 0x3a),
        bad: rgb(0xba, 0x1a, 0x1a),
        accent: ACCENT,
        accent_text: rgb(0x1a, 0x5f, 0xb4),
        selection: rgb(0xd6, 0xe5, 0xf9),
        primary: rgb(0x1a, 0x6d, 0xd0),
        on_primary: Color32::WHITE,
        popup: LIGHT_POPUP,
    };

    /// The theme `dark` or light, with `accent`: its neutral surfaces take a
    /// little of the accent, as Material 3 tints them with the seed colour
    /// (SET-010).
    pub(crate) fn new(dark: bool, accent: Color32) -> Palette {
        let base = if dark { Self::DARK } else { Self::LIGHT };
        let accent = Color32::from_rgb(accent.r(), accent.g(), accent.b());
        // how much of the accent each surface takes
        let tint = |c: Color32, share: f32| {
            let mixed = mix(accent, Color32::from_rgb(c.r(), c.g(), c.b()), share);
            Color32::from_rgba_unmultiplied(mixed.r(), mixed.g(), mixed.b(), c.a())
        };
        let unmultiplied = |c: Color32| {
            let [r, g, b, a] = c.to_srgba_unmultiplied();
            Color32::from_rgba_unmultiplied(r, g, b, a)
        };
        let (page, rail, card, dialog, placeholder) = if dark {
            (
                tint(base.page, 0.05),
                tint(base.rail, 0.07),
                tint(unmultiplied(base.card), 0.07),
                tint(base.dialog, 0.08),
                tint(base.placeholder, 0.10),
            )
        } else {
            (
                tint(base.page, 0.03),
                tint(base.rail, 0.06),
                tint(unmultiplied(base.card), 0.06),
                tint(base.dialog, 0.04),
                tint(base.placeholder, 0.08),
            )
        };
        let worst_card = worst_background(card, base.backdrop, dark);
        // the library's cards on its page
        let tile = over(base.row, page);
        let (primary, on_primary) = primary_colors(accent, dark);
        Palette {
            page,
            rail,
            card,
            dialog,
            placeholder,
            accent,
            accent_text: readable(accent, &[worst_card, tile, rail, dialog], dark),
            // the accent's tonal container: faint enough for the text to read
            selection: if dark {
                mix(accent, page, 0.32)
            } else {
                mix(accent, Color32::WHITE, 0.22)
            },
            primary,
            on_primary,
            popup: PopupColors {
                cursor: accent,
                ..base.popup
            },
            ..base
        }
    }
}

/// The named palettes (THM-001).
#[cfg(test)]
const NAMES: [&str; 3] = ["", "oled", "high-contrast"];

impl Palette {
    /// A named palette: `oled` (always dark, pure black surfaces),
    /// `high-contrast`, or the default for anything else (THM-001).
    pub(crate) fn named(name: &str, dark: bool, accent: Color32) -> Palette {
        match name {
            "oled" => {
                let mut p = Palette::new(true, accent);
                let black = Color32::BLACK;
                p.backdrop = rgba(0, 0, 0, 0xe0);
                p.page = black;
                p.rail = rgb(0x0e, 0x0e, 0x10);
                p.card = rgba(0, 0, 0, 0xfa);
                p.dialog = black;
                p.placeholder = rgb(0x1c, 0x1c, 0x1f);
                p.popup.panel = rgba(0, 0, 0, 0xfa);
                p.popup.section = rgb(0x1c, 0x1c, 0x1f);
                p.popup.header_cell = rgb(0x1c, 0x1c, 0x1f);
                p.popup.chip = rgb(0x26, 0x26, 0x2a);
                let tile = over(p.row, p.page);
                p.accent_text = readable(accent, &[p.card, tile, p.rail, p.dialog], true);
                p
            }
            "high-contrast" => {
                let mut p = Palette::new(dark, accent);
                if dark {
                    p.backdrop = rgba(0, 0, 0, 0xf0);
                    p.page = Color32::BLACK;
                    p.rail = Color32::BLACK;
                    p.card = rgba(0, 0, 0, 0xfc);
                    p.dialog = Color32::BLACK;
                    p.text = Color32::WHITE;
                    p.details = rgb(0xe0, 0xe0, 0xe0);
                    p.off = rgb(0xc8, 0xc8, 0xc8);
                    p.good = rgb(0x9f, 0xff, 0xb0);
                    p.bad = rgb(0xff, 0xc0, 0x80);
                    p.card_border = rgba(0xff, 0xff, 0xff, 0xa0);
                    p.popup.panel = rgba(0, 0, 0, 0xfc);
                    p.popup.panel_border = Color32::WHITE;
                    p.popup.text = Color32::WHITE;
                    p.popup.weak = rgb(0xe0, 0xe0, 0xe0);
                    p.popup.marker = rgb(0xe0, 0xe0, 0xe0);
                    p.popup.link = rgb(0xa8, 0xd0, 0xff);
                    p.popup.reading = rgb(0xc0, 0xdc, 0xff);
                    p.popup.section_text = Color32::WHITE;
                } else {
                    p.backdrop = rgba(0, 0, 0, 0xa0);
                    p.page = Color32::WHITE;
                    p.rail = Color32::WHITE;
                    p.card = rgba(0xff, 0xff, 0xff, 0xfe);
                    p.dialog = Color32::WHITE;
                    p.text = Color32::BLACK;
                    p.details = rgb(0x2a, 0x2a, 0x2e);
                    p.off = rgb(0x40, 0x40, 0x46);
                    p.good = rgb(0x0b, 0x4a, 0x24);
                    p.bad = rgb(0x6c, 0x2c, 0x00);
                    p.card_border = rgba(0, 0, 0, 0xa0);
                    p.popup.panel = rgba(0xff, 0xff, 0xff, 0xfe);
                    p.popup.panel_border = Color32::BLACK;
                    p.popup.text = Color32::BLACK;
                    p.popup.headword = Color32::BLACK;
                    p.popup.weak = rgb(0x2a, 0x2a, 0x2e);
                    p.popup.marker = rgb(0x2a, 0x2a, 0x2e);
                    p.popup.link = rgb(0x0b, 0x3d, 0x7a);
                    p.popup.reading = rgb(0x0b, 0x2c, 0x60);
                    p.popup.section_text = Color32::BLACK;
                }
                let tile = over(p.row, p.page);
                let card = worst_background(p.card, p.backdrop, dark);
                p.accent_text = readable_at(accent, &[card, tile, p.rail, p.dialog], dark, 7.0);
                p
            }
            _ => Palette::new(dark, accent),
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
    readable_at(accent, backgrounds, dark, 4.5)
}

/// `accent` made lighter or darker until it reads at `ratio`.
fn readable_at(accent: Color32, backgrounds: &[Color32], dark: bool, ratio: f32) -> Color32 {
    let toward = if dark { Color32::WHITE } else { Color32::BLACK };
    let mut color = Color32::from_rgb(accent.r(), accent.g(), accent.b());
    let mut t = 1.0;
    while backgrounds.iter().any(|b| contrast(color, *b) < ratio) && t > 0.0 {
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
    apply_named(ctx, "", dark, accent);
}

/// Puts a named palette in use (THM-001); OLED is always dark.
pub(crate) fn apply_named(ctx: &egui::Context, name: &str, dark: bool, accent: Color32) {
    let palette = Palette::named(name, dark, accent);
    let dark = palette.dark;
    CURRENT.with(|c| c.set(palette));
    let theme = if dark {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    };
    ctx.set_theme(theme);
    ctx.style_mut_of(theme, |style| material(style, &palette));
}

/// A pill: the corners as round as the shape allows (epaint clamps the
/// radius to half the shorter side).
pub(crate) const PILL: u8 = 255;

/// The corners of a dialog and of a menu (SET-010).
pub(crate) const DIALOG_RADIUS: u8 = 28;
const MENU_RADIUS: u8 = 16;

/// The corners of a box of text over several lines: a pill would round
/// them away.
const BOX_RADIUS: u8 = 12;

/// egui's widgets in the manner of Fushi 3's Material 3 Expressive
/// (SET-010): pill-shaped buttons, fields and tabs on a tonal fill that
/// brightens under the pointer; what is chosen on the accent's tonal
/// container; menus and dialogs with large round corners; taller targets.
fn material(style: &mut egui::Style, p: &Palette) {
    use egui::{CornerRadius, Shadow, Stroke};
    // the fills: the text's colour, faint, so that they read on any surface
    let ink = Color32::from_rgb(p.text.r(), p.text.g(), p.text.b());
    let fill = |a: u8| Color32::from_rgba_unmultiplied(ink.r(), ink.g(), ink.b(), a);
    let v = &mut style.visuals;
    v.panel_fill = p.page;
    v.window_fill = p.dialog;
    v.window_stroke = Stroke::new(1.0, p.card_border);
    v.window_corner_radius = CornerRadius::same(DIALOG_RADIUS);
    v.menu_corner_radius = CornerRadius::same(MENU_RADIUS);
    v.window_shadow = Shadow {
        offset: [0, 6],
        blur: 24,
        spread: 0,
        color: rgba(0, 0, 0, if p.dark { 0x78 } else { 0x30 }),
    };
    v.popup_shadow = Shadow {
        offset: [0, 3],
        blur: 12,
        spread: 0,
        color: rgba(0, 0, 0, if p.dark { 0x60 } else { 0x28 }),
    };
    // text fields and the inside of sliders: a step up from the page
    let field = over(fill(if p.dark { 0x14 } else { 0x0c }), p.page);
    v.extreme_bg_color = field;
    v.text_edit_bg_color = Some(field);
    v.faint_bg_color = over(fill(0x08), p.page);
    v.code_bg_color = field;
    v.selection.bg_fill = p.selection;
    v.selection.stroke = Stroke::new(1.0, p.text);
    v.hyperlink_color = p.accent_text;
    v.text_cursor.stroke = Stroke::new(2.0, p.accent_text);
    v.warn_fg_color = p.bad;
    v.error_fg_color = p.bad;
    v.window_highlight_topmost = false;
    // sliders as Material 3's: a filled track up to a narrow upright handle
    v.slider_trailing_fill = true;
    v.handle_shape = egui::style::HandleShape::Rect { aspect_ratio: 0.4 };
    let pill = CornerRadius::same(PILL);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p.page;
    w.noninteractive.weak_bg_fill = p.page;
    w.noninteractive.bg_stroke = Stroke::new(1.0, fill(0x18));
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
    w.noninteractive.corner_radius = CornerRadius::same(BOX_RADIUS);
    for (state, a) in [
        (&mut w.inactive, 0x16),
        (&mut w.hovered, 0x24),
        (&mut w.active, 0x38),
        (&mut w.open, 0x24),
    ] {
        state.weak_bg_fill = fill(a);
        state.bg_fill = fill(a);
        state.bg_stroke = Stroke::NONE;
        state.fg_stroke = Stroke::new(1.0, p.text);
        state.corner_radius = pill;
        state.expansion = 0.0;
    }
    // a text field's outline, and the focus in the accent
    w.inactive.bg_stroke = Stroke::new(1.0, fill(0x14));
    w.hovered.bg_stroke = Stroke::new(1.0, fill(0x38));
    w.active.bg_stroke = Stroke::new(2.0, p.accent_text);
    // checkboxes and sliders: the accent when on
    w.active.bg_fill = p.accent;
    let s = &mut style.spacing;
    s.button_padding = egui::vec2(14.0, 6.0);
    s.interact_size.y = 30.0;
    s.item_spacing = egui::vec2(8.0, 6.0);
    s.menu_margin = egui::Margin::same(8);
    s.window_margin = egui::Margin::same(20);
    s.icon_width = 16.0;
    s.icon_width_inner = 10.0;
}

/// Adds `widget`, a box of text over several lines, with the corners of a
/// box rather than a pill's, which would round it away (SET-010).
pub(crate) fn add_boxy(ui: &mut egui::Ui, widget: impl egui::Widget) -> egui::Response {
    ui.scope(|ui| {
        boxy(ui);
        ui.add(widget)
    })
    .inner
}

/// Gives `ui` the corners of a box of text over several lines.
fn boxy(ui: &mut egui::Ui) {
    let radius = egui::CornerRadius::same(BOX_RADIUS);
    let w = &mut ui.visuals_mut().widgets;
    for state in [&mut w.inactive, &mut w.hovered, &mut w.active, &mut w.open] {
        state.corner_radius = radius;
    }
}

/// The fill of a page's main action and its text, from `accent` (SET-010):
/// in the dark theme the accent made light with a dark text, as Material 3's
/// primary at tone 80; in the light one the accent darkened until white
/// reads on it at 4.5:1, or black on an accent too light for white.
pub(crate) fn primary_colors(accent: Color32, dark: bool) -> (Color32, Color32) {
    let accent = Color32::from_rgb(accent.r(), accent.g(), accent.b());
    if dark {
        let ink = mix(accent, Color32::BLACK, 0.2);
        let mut t = 0.55;
        let mut fill = mix(accent, Color32::WHITE, t);
        while contrast(fill, ink) < 4.5 && t > 0.0 {
            t -= 0.05;
            fill = mix(accent, Color32::WHITE, t);
        }
        return (fill, ink);
    }
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

/// A page's main action, filled with the primary colour (SET-010).
pub(crate) fn suggested(text: impl Into<String>) -> egui::Button<'static> {
    let p = current();
    egui::Button::new(egui::RichText::new(text.into()).color(p.on_primary)).fill(p.primary)
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
        for (name, dark) in NAMES.iter().flat_map(|n| [(*n, true), (*n, false)]) {
            for accent in accents {
                let p = Palette::named(name, dark, accent);
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
                    ("accent/dialog", p.accent_text, vec![p.dialog]),
                    ("text/rail", p.text, vec![p.rail]),
                    ("details/rail", p.details, vec![p.rail]),
                    ("accent/rail", p.accent_text, vec![p.rail]),
                    ("primary", p.on_primary, vec![p.primary]),
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
                // THM-001: the main texts of High contrast read at 7:1
                let main = [
                    "text/card",
                    "details/card",
                    "text/page",
                    "text/dialog",
                    "text/panel",
                    "headword/panel",
                    "accent/card",
                ];
                for (what, text, layers) in pairs.into_iter().chain(popup) {
                    let ratio = worst(text, &layers);
                    let needed = if name == "high-contrast" && main.contains(&what) {
                        7.0
                    } else {
                        4.5
                    };
                    assert!(
                        ratio >= needed,
                        "{name} {what}, dark {dark}, accent {accent:?}: {ratio:.2}"
                    );
                }
            }
        }
    }

    // T-SET-009, T-SET-010: the main action reads on its fill, whatever the accent
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
            for dark in [true, false] {
                let (fill, ink) = primary_colors(accent, dark);
                assert!(
                    contrast(fill, ink) >= 4.5,
                    "{accent:?}, dark {dark}: {fill:?} {ink:?}"
                );
            }
        }
        // GNOME's blue is darkened a little for white text in the light
        // theme, made light under dark text in the dark one
        assert_ne!(primary_colors(ACCENT, false).0, ACCENT);
        let (fill, ink) = primary_colors(ACCENT, true);
        assert!(luminance(fill) > luminance(ink));
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
