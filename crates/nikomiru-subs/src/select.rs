// SPDX-License-Identifier: GPL-3.0-only

//! Which subtitles a video gets: external files next to it (SUB-006) and the
//! automatic choice (SUB-008).

use std::path::{Path, PathBuf};

use crate::cue::Format;

/// Spellings of the languages that subtitle files and tracks use, by their
/// ISO 639-1 code.
const LANGUAGES: [(&str, &[&str]); 12] = [
    ("ja", &["ja", "jp", "jpn", "japanese", "日本語"]),
    ("en", &["en", "eng", "english"]),
    ("it", &["it", "ita", "italian", "italiano"]),
    ("zh", &["zh", "chi", "zho", "chinese", "chs", "cht", "中文"]),
    ("ko", &["ko", "kor", "korean", "한국어"]),
    ("es", &["es", "spa", "spanish", "español"]),
    ("fr", &["fr", "fre", "fra", "french", "français"]),
    ("de", &["de", "ger", "deu", "german", "deutsch"]),
    ("pt", &["pt", "por", "portuguese", "português"]),
    ("ru", &["ru", "rus", "russian", "русский"]),
    ("ar", &["ar", "ara", "arabic"]),
    ("id", &["id", "ind", "indonesian"]),
];

/// The ISO 639-1 code of a language tag (`jpn`, `ja-JP`, `Japanese`…), or the
/// tag itself, lower-cased, if it is not known.
pub fn language_code(tag: &str) -> String {
    let lower = tag.trim().to_lowercase();
    let primary = lower.split(['-', '_']).next().unwrap_or("");
    LANGUAGES
        .iter()
        .find(|(_, names)| names.contains(&primary) || names.contains(&lower.as_str()))
        .map_or(primary.to_string(), |(code, _)| (*code).to_string())
}

/// Whether `tag` is the language `preferred` (both in any spelling).
pub fn same_language(preferred: &str, tag: &str) -> bool {
    !tag.trim().is_empty() && language_code(preferred) == language_code(tag)
}

/// A subtitle file next to the video.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct External {
    /// The file.
    pub path: PathBuf,
    /// Its format.
    pub format: Format,
    /// The language suffix (`ja` in `Ep01.ja.srt`), if any.
    pub language: Option<String>,
}

fn looks_like_language(tag: &str) -> bool {
    (2..=12).contains(&tag.chars().count())
        && tag
            .chars()
            .all(|c| c.is_alphabetic() || c == '-' || c == '_')
}

/// SUB-006: among `candidates`, the subtitle files of `video`: the same base
/// name (any case) with `.srt .ass .ssa .vtt`, optionally with a language
/// suffix (`.ja.srt`, `.jpn.ass`). Sorted by file name.
pub fn external_files(video: &Path, candidates: &[PathBuf]) -> Vec<External> {
    let Some(base) = video
        .file_stem()
        .map(|s| s.to_string_lossy().to_lowercase())
    else {
        return Vec::new();
    };
    let mut out: Vec<External> = candidates
        .iter()
        .filter_map(|path| {
            let format = Format::from_extension(&path.extension()?.to_string_lossy())?;
            let stem = path.file_stem()?.to_string_lossy().to_lowercase();
            let language = if stem == base {
                None
            } else {
                let suffix = stem.strip_prefix(&base)?.strip_prefix('.')?;
                if !looks_like_language(suffix) {
                    return None;
                }
                Some(suffix.to_string())
            };
            Some(External {
                path: path.clone(),
                format,
                language,
            })
        })
        .collect();
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

/// An embedded subtitle track, as the player lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddedTrack {
    /// mpv track id.
    pub id: i64,
    /// Language tag.
    pub language: Option<String>,
    /// Title.
    pub title: Option<String>,
    /// Text (SRT, ASS, WebVTT…): Nikomiru draws it; otherwise mpv does (PGS…).
    pub text: bool,
}

/// What to show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
    /// An external file.
    External(PathBuf),
    /// An embedded track.
    Embedded(i64),
}

/// SUB-008: (1) the saved choice, if still available; (2) an external file in
/// the preferred language (then one without a language suffix); (3) an embedded
/// track in the preferred language (by tag, or a title that names it), text
/// tracks first; (4) none.
pub fn choose(
    saved: Option<&Choice>,
    externals: &[External],
    embedded: &[EmbeddedTrack],
    preferred: &str,
) -> Option<Choice> {
    if let Some(saved) = saved {
        let available = match saved {
            Choice::External(p) => externals.iter().any(|e| &e.path == p),
            Choice::Embedded(id) => embedded.iter().any(|t| t.id == *id),
        };
        if available {
            return Some(saved.clone());
        }
    }
    let tagged = externals.iter().find(|e| {
        e.language
            .as_deref()
            .is_some_and(|l| same_language(preferred, l))
    });
    let untagged = externals.iter().find(|e| e.language.is_none());
    if let Some(e) = tagged.or(untagged) {
        return Some(Choice::External(e.path.clone()));
    }
    // in a title only full names count ("Japanese", "日本語"), not codes like "ja"
    let code = language_code(preferred);
    let full_names: Vec<&str> = LANGUAGES
        .iter()
        .filter(|(c, _)| *c == code)
        .flat_map(|(_, names)| names.iter().copied())
        .filter(|n| n.len() > 3)
        .collect();
    let names_it = |t: &EmbeddedTrack| {
        t.language
            .as_deref()
            .is_some_and(|l| same_language(preferred, l))
            || t.title.as_deref().is_some_and(|title| {
                let lower = title.to_lowercase();
                full_names.iter().any(|n| lower.contains(n))
            })
    };
    embedded
        .iter()
        .filter(|t| names_it(t))
        .max_by_key(|t| (t.text, std::cmp::Reverse(t.id)))
        .map(|t| Choice::Embedded(t.id))
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-SUB-006
    #[test]
    fn external_files_by_base_name_and_language_suffix() {
        let names = [
            "Ep01.ja.srt",
            "Ep01.jpn.ass",
            "Ep011.srt",
            "ep01.SRT",
            "Ep01.mkv",
            "Ep01.ja.txt",
            "Ep01.something long.srt",
        ];
        let paths: Vec<PathBuf> = names.iter().map(|n| PathBuf::from("/v").join(n)).collect();
        let found: Vec<(String, Option<String>)> = external_files(Path::new("/v/Ep01.mkv"), &paths)
            .into_iter()
            .map(|e| {
                (
                    e.path.file_name().unwrap().to_string_lossy().into_owned(),
                    e.language,
                )
            })
            .collect();
        assert_eq!(
            found,
            [
                ("Ep01.ja.srt".to_string(), Some("ja".to_string())),
                ("Ep01.jpn.ass".to_string(), Some("jpn".to_string())),
                ("ep01.SRT".to_string(), None),
            ]
        );
    }

    fn ext(name: &str, language: Option<&str>) -> External {
        External {
            path: PathBuf::from(name),
            format: Format::Srt,
            language: language.map(str::to_string),
        }
    }

    fn track(id: i64, language: Option<&str>, title: Option<&str>, text: bool) -> EmbeddedTrack {
        EmbeddedTrack {
            id,
            language: language.map(str::to_string),
            title: title.map(str::to_string),
            text,
        }
    }

    // T-SUB-008
    #[test]
    fn the_four_rules_in_order() {
        let externals = [ext("a.en.srt", Some("en")), ext("a.ja.srt", Some("ja"))];
        let embedded = [
            track(1, Some("eng"), None, true),
            track(2, Some("jpn"), None, false),
            track(3, None, Some("Japanese (CC)"), true),
        ];
        // (1) the saved choice wins while it exists
        let saved = Choice::Embedded(1);
        assert_eq!(
            choose(Some(&saved), &externals, &embedded, "ja"),
            Some(saved)
        );
        let gone = Choice::External(PathBuf::from("gone.srt"));
        // (2) external in the preferred language
        assert_eq!(
            choose(Some(&gone), &externals, &embedded, "ja"),
            Some(Choice::External(PathBuf::from("a.ja.srt")))
        );
        // (3) embedded: the text track named "Japanese" before the graphic jpn one
        assert_eq!(
            choose(None, &externals[..1], &embedded, "ja"),
            Some(Choice::Embedded(3))
        );
        assert_eq!(
            choose(None, &[], &embedded[..2], "jpn"),
            Some(Choice::Embedded(2))
        );
        // (4) none
        assert_eq!(choose(None, &externals[..1], &embedded[..1], "ja"), None);
        // an external without a suffix is taken when none has the language
        assert_eq!(
            choose(None, &[ext("a.srt", None)], &embedded, "ja"),
            Some(Choice::External(PathBuf::from("a.srt")))
        );
    }

    #[test]
    fn language_spellings() {
        assert_eq!(language_code("ja-JP"), "ja");
        assert_eq!(language_code("JPN"), "ja");
        assert!(same_language("ja", "日本語"));
        let titled = [track(5, None, Some("字幕 日本語"), true)];
        assert_eq!(choose(None, &[], &titled, "ja"), Some(Choice::Embedded(5)));
        assert!(!same_language("ja", ""));
        assert_eq!(language_code("tlh"), "tlh");
    }
}
