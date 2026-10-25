// SPDX-License-Identifier: GPL-3.0-only

//! The settings (⚙ in the bar, SET-001, SET-008): one page per tab, the tabs
//! down the left side of each, the tab's title, its buttons and its last
//! message at the top, its content below.

use egui::{
    Align, Layout, Rect, RichText, ScrollArea, Stroke, StrokeKind, Ui, UiBuilder, pos2, vec2,
};

use super::dict_page::full_screen;
use super::{palette, theme};

/// A tab of the settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsTab {
    /// Language, theme, scale; the seek step, the next episode.
    #[default]
    General,
    /// The mpv options and presets (PLY-016, PLY-018).
    Player,
    /// The subtitles' size, languages and obscuring.
    Subtitles,
    /// The subtitle services, their keys and the proxy (MET-011).
    Services,
    /// The dictionaries, their order and their styles (DIC-*).
    Dictionaries,
    /// The lookups and the popups.
    Lookup,
    /// The word audio sources (AUD-001).
    Audio,
    /// Mining to Anki.
    Anki,
    /// The keyboard shortcuts (KEY-001).
    Shortcuts,
    /// ffmpeg, the folders, the logs.
    Advanced,
    /// The version and the licenses.
    About,
}

impl SettingsTab {
    /// Every tab, in the order shown.
    pub const ALL: [SettingsTab; 11] = [
        Self::General,
        Self::Player,
        Self::Subtitles,
        Self::Services,
        Self::Dictionaries,
        Self::Lookup,
        Self::Audio,
        Self::Anki,
        Self::Shortcuts,
        Self::Advanced,
        Self::About,
    ];

    /// The keys of `config.toml` it saves and resets (SET-006), for the
    /// tabs that save through [`Command::SettingsSave`](crate::ui::Command).
    pub fn section(self) -> Option<nikomiru_core::config::Section> {
        use nikomiru_core::config::Section;
        match self {
            Self::General => Some(Section::General),
            Self::Subtitles => Some(Section::Subtitles),
            Self::Lookup => Some(Section::Lookup),
            Self::Shortcuts => Some(Section::Shortcuts),
            Self::Advanced => Some(Section::Advanced),
            _ => None,
        }
    }

    /// Its name, for the test harness and the scroll position.
    pub fn key(self) -> &'static str {
        match self {
            Self::General => "general",
            Self::Player => "player",
            Self::Subtitles => "subtitles",
            Self::Services => "services",
            Self::Dictionaries => "dictionaries",
            Self::Lookup => "lookup",
            Self::Audio => "audio",
            Self::Anki => "anki",
            Self::Shortcuts => "shortcuts",
            Self::Advanced => "advanced",
            Self::About => "about",
        }
    }

    /// The tab of a name of the test harness.
    #[cfg_attr(not(feature = "test-harness"), allow(dead_code))]
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.key() == key)
    }

    fn label(self) -> &'static str {
        match self {
            Self::General => "settings-general",
            Self::Player => "settings-player",
            Self::Subtitles => "settings-subtitles",
            Self::Services => "settings-services",
            Self::Dictionaries => "settings-dictionaries",
            Self::Lookup => "settings-lookup",
            Self::Audio => "settings-audio",
            Self::Anki => "settings-anki",
            Self::Shortcuts => "settings-shortcuts",
            Self::Advanced => "settings-advanced",
            Self::About => "settings-about",
        }
    }
}

/// What a page asks for after a frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Leave {
    /// It stays open.
    #[default]
    Stay,
    /// It closes.
    Close,
    /// It closes, and this tab of the settings opens.
    To(SettingsTab),
}

/// The width of the column of the tabs.
const TABS: f32 = 190.0;

/// The page of the tab `current`: `content` gets the row of the buttons at
/// the top (laid out from the right, left of Close) and the body, which
/// scrolls. `status` is the last message, and whether it is good news.
pub(crate) fn page(
    ctx: &egui::Context,
    current: SettingsTab,
    tr: &dyn Fn(&str) -> String,
    status: Option<&(String, bool)>,
    content: impl FnOnce(&mut Ui, &mut Ui),
) -> Leave {
    let p = palette::current();
    let screen = ctx.content_rect();
    let size = vec2(
        1040.0_f32.min(screen.width() - 32.0).max(0.0),
        760.0_f32.min(screen.height() - 32.0).max(0.0),
    );
    let mut leave = Leave::Stay;
    full_screen(ctx, "settings-page", p.backdrop, |ui| {
        let card = Rect::from_center_size(screen.center(), size);
        ui.painter().rect(
            card,
            10.0,
            p.card,
            Stroke::new(1.0, p.card_border),
            StrokeKind::Inside,
        );
        let inner = card.shrink(16.0);
        let split = (inner.left() + TABS).min(inner.right());
        let side = Rect::from_min_max(inner.min, pos2(split, inner.bottom()));
        let main = Rect::from_min_max(
            pos2((split + 17.0).min(inner.right()), inner.top()),
            inner.max,
        );
        ui.painter().vline(
            split + 8.0,
            inner.y_range(),
            Stroke::new(1.0, p.card_border),
        );

        // the tabs
        let mut tabs = ui.new_child(
            UiBuilder::new()
                .max_rect(side)
                .layout(Layout::top_down_justified(Align::Min)),
        );
        tabs.visuals_mut().override_text_color = Some(p.text);
        tabs.spacing_mut().item_spacing.y = 4.0;
        tabs.spacing_mut().button_padding = vec2(10.0, 6.0);
        for tab in SettingsTab::ALL {
            let text = RichText::new(tr(tab.label())).size(16.0);
            if tabs.selectable_label(tab == current, text).clicked() && tab != current {
                leave = Leave::To(tab);
            }
        }

        // the title, the buttons, the message, the body
        let mut ui = ui.new_child(
            UiBuilder::new()
                .max_rect(main)
                .layout(Layout::top_down(Align::Min)),
        );
        ui.visuals_mut().override_text_color = Some(p.text);
        ui.spacing_mut().item_spacing.y = 10.0;
        let mut buttons = None;
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(tr(current.label()))
                    .size(20.0)
                    .family(theme::bold()),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button(tr("settings-close")).clicked() {
                    leave = Leave::Close;
                }
                buttons = Some(
                    ui.new_child(
                        UiBuilder::new()
                            .max_rect(ui.available_rect_before_wrap())
                            .layout(Layout::right_to_left(Align::Center)),
                    ),
                );
            });
        });
        if let Some((text, ok)) = status {
            let color = if *ok { p.good } else { p.bad };
            ui.add(egui::Label::new(RichText::new(text).color(color)).wrap());
        }
        let Some(mut buttons) = buttons else {
            return;
        };
        ScrollArea::vertical()
            .id_salt(("settings", current.key()))
            .auto_shrink([false, false])
            .show(&mut ui, |ui| content(&mut buttons, ui));
    });
    leave
}
