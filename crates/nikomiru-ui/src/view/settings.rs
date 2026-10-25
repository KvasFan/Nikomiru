// SPDX-License-Identifier: GPL-3.0-only

//! The settings (⚙ in the bar, SET-008): one page per tab, Player, Subtitle
//! services, Word audio and Anki, with the tabs at the top of each.

use egui::{RichText, Ui};

use super::theme;

/// A tab of the settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsTab {
    /// The mpv options and presets (PLY-016, PLY-018).
    #[default]
    Player,
    /// The subtitle services, their keys and the proxy (MET-011).
    Services,
    /// The word audio sources (AUD-001).
    Audio,
    /// Mining to Anki.
    Anki,
}

impl SettingsTab {
    /// Every tab, in the order shown.
    pub const ALL: [SettingsTab; 4] = [Self::Player, Self::Services, Self::Audio, Self::Anki];

    /// Its name for the test harness.
    #[cfg_attr(not(feature = "test-harness"), allow(dead_code))]
    pub fn key(self) -> &'static str {
        match self {
            Self::Player => "player",
            Self::Services => "services",
            Self::Audio => "audio",
            Self::Anki => "anki",
        }
    }

    /// The tab of a name of the test harness.
    #[cfg_attr(not(feature = "test-harness"), allow(dead_code))]
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.key() == key)
    }

    fn label(self) -> &'static str {
        match self {
            Self::Player => "settings-player",
            Self::Services => "settings-services",
            Self::Audio => "settings-audio",
            Self::Anki => "settings-anki",
        }
    }
}

/// What a page asks for after a frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Leave {
    /// It stays open.
    Stay,
    /// It closes.
    Close,
    /// It closes, and this tab of the settings opens.
    To(SettingsTab),
}

/// The tabs, `current` selected, in place of a page's title; the tab chosen.
pub(crate) fn tabs(
    ui: &mut Ui,
    current: SettingsTab,
    tr: &dyn Fn(&str) -> String,
) -> Option<SettingsTab> {
    let mut chosen = None;
    for tab in SettingsTab::ALL {
        let text = RichText::new(tr(tab.label()))
            .size(20.0)
            .family(theme::bold());
        if ui.selectable_label(tab == current, text).clicked() && tab != current {
            chosen = Some(tab);
        }
    }
    chosen
}
