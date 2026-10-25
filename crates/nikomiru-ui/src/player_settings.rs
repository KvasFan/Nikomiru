// SPDX-License-Identifier: GPL-3.0-only

//! The Player settings page (PLY-016, PLY-018): the mpv option set edited
//! with controls and as raw options, presets in `mpv.conf` format saved,
//! loaded with a preview of every line, imported and exported.

use std::path::PathBuf;

use nikomiru_player::options::{Ignored, Line};
use nikomiru_player::settings::{OptionSet, control_of};

/// What happens to one line of a preset or an imported file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineStatus {
    /// It sets a control.
    Control,
    /// It goes to the raw options.
    Raw,
    /// Not applied, and why (translated).
    Ignored(String),
    /// Not valid, and why.
    Invalid(String),
}

/// One line of the preview.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewLine {
    /// Its number in the file.
    pub line: usize,
    /// The option or the section.
    pub text: String,
    /// What happens to it.
    pub status: LineStatus,
}

/// A preset or a file being loaded (PLY-016): nothing changes until the user
/// applies it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preview {
    /// Its name, or the file's.
    pub source: String,
    /// The preset's name, when it is one: remembered as `mpv_preset`.
    pub preset: Option<String>,
    /// The set it gives.
    pub set: OptionSet,
    /// Every line.
    pub lines: Vec<PreviewLine>,
}

impl Preview {
    /// The preview of `text`; `reason` translates why a line is ignored.
    pub fn of(
        source: String,
        preset: Option<String>,
        text: &str,
        reason: &dyn Fn(&Ignored) -> String,
    ) -> Self {
        let (set, parsed) = OptionSet::from_mpv_conf(text);
        let lines = parsed
            .iter()
            .map(|l| match l {
                Line::Apply { line, option } => PreviewLine {
                    line: *line,
                    text: format!("{}={}", option.key, option.value),
                    status: if control_of(&option.key).is_some() {
                        LineStatus::Control
                    } else {
                        LineStatus::Raw
                    },
                },
                Line::Ignore {
                    line,
                    name,
                    reason: why,
                } => PreviewLine {
                    line: *line,
                    text: name.clone(),
                    status: LineStatus::Ignored(reason(why)),
                },
                Line::Invalid { line, error } => PreviewLine {
                    line: *line,
                    text: String::new(),
                    status: LineStatus::Invalid(error.clone()),
                },
            })
            .collect();
        Self {
            source,
            preset,
            set,
            lines,
        }
    }
}

/// The page in the session.
#[derive(Debug, Default)]
pub struct PlayerPage {
    /// The saved presets, by name.
    pub presets: Vec<String>,
    /// A preset or a file waiting to be applied.
    pub preview: Option<Preview>,
    /// The last message, and whether it is good news.
    pub status: Option<(String, bool)>,
    /// Sets applied so far: the page takes the new settings on each.
    pub applied: u64,
}

/// A file to write off the UI thread: a preset or an export.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileWrite {
    /// Where.
    pub path: PathBuf,
    /// What.
    pub text: String,
    /// Its permissions.
    pub mode: u32,
    /// The preset's name, or `None` for an export.
    pub preset: Option<String>,
}
