// SPDX-License-Identifier: GPL-3.0-only

//! Online services chosen by the user ([ADR 0008]): subtitles from Jimaku and
//! OpenSubtitles, searched by hand or automatically (MET-003, MET-004,
//! MET-007…MET-010). AniList and Jikan metadata come with M7.
//!
//! [ADR 0008]: ../../../docs/adr/0008-online-subtitles.md

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod archive;
pub mod audio;
pub mod error;
#[cfg(feature = "test-support")]
pub mod fake;
pub mod hash;
pub mod http;
pub mod jimaku;
pub mod language;
pub mod opensubtitles;
pub mod rank;
pub mod search;
pub mod store;

pub use error::OnlineError;
pub use hash::VideoHash;

/// A provider of subtitles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ProviderId {
    /// jimaku.cc (MET-003).
    Jimaku,
    /// opensubtitles.com (MET-004).
    OpenSubtitles,
}

impl ProviderId {
    /// Every provider, in the order used when the settings name none.
    pub const ALL: [ProviderId; 2] = [ProviderId::Jimaku, ProviderId::OpenSubtitles];

    /// Its name in `[online] providers` (MET-011).
    pub fn key(self) -> &'static str {
        match self {
            ProviderId::Jimaku => "jimaku",
            ProviderId::OpenSubtitles => "opensubtitles",
        }
    }

    /// The provider of a name in `[online] providers`, in any case.
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|p| p.key().eq_ignore_ascii_case(key.trim()))
    }

    /// The providers `[online] providers` names, in its order, once each;
    /// unknown names are left out.
    pub fn from_keys(keys: &[String]) -> Vec<Self> {
        let mut out = Vec::new();
        for p in keys.iter().filter_map(|k| Self::from_key(k)) {
            if !out.contains(&p) {
                out.push(p);
            }
        }
        out
    }

    /// The name shown.
    pub fn name(self) -> &'static str {
        match self {
            ProviderId::Jimaku => "Jimaku",
            ProviderId::OpenSubtitles => "OpenSubtitles",
        }
    }
}

/// What to search for (ADR 0008 §2).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SearchRequest {
    /// The title.
    pub title: String,
    /// The season.
    pub season: Option<u32>,
    /// The episode; `None` for a film.
    pub episode: Option<u32>,
    /// The video's OpenSubtitles hash.
    pub hash: Option<VideoHash>,
    /// The language wanted (ISO 639-1, `ja`).
    pub language: String,
    /// The providers in the user's order of preference (MET-011), which
    /// breaks the ties of the ranking; empty: [`ProviderId::ALL`]'s order.
    pub providers: Vec<ProviderId>,
}

/// What a file of a provider is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FileKind {
    /// SRT, ASS, SSA or WebVTT.
    Subtitle,
    /// A ZIP archive, with subtitle files inside.
    Zip,
    /// RAR, 7z or anything else: listed, not usable.
    Unusable,
}

impl FileKind {
    /// The kind of a file, from its name.
    pub fn of(file_name: &str) -> Self {
        if language::is_subtitle(file_name) {
            FileKind::Subtitle
        } else if language::extension(file_name).as_deref() == Some("zip") {
            FileKind::Zip
        } else {
            FileKind::Unusable
        }
    }
}

/// How a provider finds a file again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Locator {
    /// Its address (Jimaku).
    Url(String),
    /// Its id (OpenSubtitles).
    FileId(u64),
}

/// A file one provider offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// The provider.
    pub provider: ProviderId,
    /// How to download it.
    pub locator: Locator,
    /// Its name.
    pub file_name: String,
    /// The Jimaku entry, or the OpenSubtitles release.
    pub source: String,
    /// Its language, if known (ISO 639-1).
    pub language: Option<String>,
    /// Its episode, if known.
    pub episode: Option<u32>,
    /// What it is.
    pub kind: FileKind,
    /// Machine translated (OpenSubtitles).
    pub machine_translated: bool,
    /// Its size, if known.
    pub size: Option<u64>,
    /// The provider's own order.
    pub order: usize,
}

/// A subtitle file, downloaded or taken out of an archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubtitleFile {
    /// Its name.
    pub name: String,
    /// Its bytes, in the file's own encoding.
    pub bytes: Vec<u8>,
}

/// A provider of subtitles.
pub trait Provider: Send {
    /// Which one.
    fn id(&self) -> ProviderId;
    /// The files it has for `request`.
    fn search(&mut self, request: &SearchRequest) -> Result<Vec<Candidate>, OnlineError>;
    /// The bytes of one of its files.
    fn download(&mut self, candidate: &Candidate) -> Result<Vec<u8>, OnlineError>;
}
