// SPDX-License-Identifier: GPL-3.0-only

//! File names parsed Anitomy-style (MET-010, LIB-003): title, season and
//! episode, for the subtitle search and the library.
//!
//! A subset of Fushi `packages/fushi_engine/lib/media/video/scraper/filename_parser.dart`
//! and `video_filename_parser.dart` (commit `ac8034e`), with their test vectors.

use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

/// What a file name says.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NameInfo {
    /// The title, never empty for a non-empty name.
    pub title: String,
    /// The season, 1-based.
    pub season: Option<u32>,
    /// The episode.
    pub episode: Option<u32>,
}

/// Extensions removed from the end of a name.
const EXTENSIONS: [&str; 19] = [
    "mkv", "mp4", "avi", "webm", "m4v", "ts", "m2ts", "mts", "mov", "wmv", "flv", "mpg", "mpeg",
    "ogv", "rmvb", "srt", "ass", "ssa", "vtt",
];

fn re(pattern: &str) -> Regex {
    // the patterns are constants: a wrong one fails every test of this module
    Regex::new(pattern).unwrap_or_else(|e| panic!("bad pattern {pattern}: {e}"))
}

/// `{[s=2;e=3]}`: an explicit block (MoviePilot, Emby).
static EXPLICIT: LazyLock<Regex> = LazyLock::new(|| re(r"\{\[([^\]]+)\]\}"));
static EXPLICIT_SEASON: LazyLock<Regex> =
    LazyLock::new(|| re(r"(?i)(?:^|;)\s*(?:s|season)\s*=\s*(\d+)"));
static EXPLICIT_EPISODE: LazyLock<Regex> =
    LazyLock::new(|| re(r"(?i)(?:^|;)\s*(?:e|episode)\s*=\s*(\d+)"));
/// A bracketed block: `[…]`, `【…】`, `(…)`, `（…）`.
static BRACKET: LazyLock<Regex> =
    LazyLock::new(|| re(r"\[([^\]]*)\]|【([^】]*)】|\(([^)]*)\)|（([^）]*)）"));
/// A bare episode number in a block: `[01]`, `[12v2]`.
static BLOCK_EPISODE: LazyLock<Regex> = LazyLock::new(|| re(r"^(\d{1,4})(?:v\d)?$"));
/// Technical tokens: resolution, codecs, sources, versions.
static TECH: LazyLock<Regex> = LazyLock::new(|| {
    re(r"(?ix)^(
        \d{3,4}[pi] | \d{3,4}x\d{3,4} | [xh]\.?26[45] | hevc(-?10bit)? | avc | av1 | vp9 | xvid | divx
        | aac(\d(\.\d)?)? | flac(x\d)? | opus | ac3 | e-?ac-?3 | dts(-hd)? | truehd | ddp?(\d(\.\d)?)?
        | \d{1,2}-?bits? | bd(rip)? | blu-?ray | bdmv | web(-?dl|-?rip)? | webrip | hdtv | dvd(rip)?
        | remux | hdr(10)? | sdr | dv | netflix | nf | amzn | cr | uncensored | dual(-?audio)?
        | multi(-?subs?)? | v\d | 10bit | 8bit | ja(\[cc\])? | jpn | eng
    )$")
});
static SEASON_EPISODE: LazyLock<Regex> =
    LazyLock::new(|| re(r"(?i)(?:^|[\s\[])s(\d{1,2})\s?e(\d{1,4})(?:v\d)?\b"));
static CJK_EPISODE: LazyLock<Regex> = LazyLock::new(|| re(r"第\s*(\d{1,4})\s*[話话集回]"));
static EP_TOKEN: LazyLock<Regex> =
    LazyLock::new(|| re(r"(?i)(?:^|\s)(?:episode|ep|e)\.?\s?(\d{1,4})(?:v\d)?(?:\s|$)"));
static HASH_EPISODE: LazyLock<Regex> =
    LazyLock::new(|| re(r"(?:^|\s)#\s?(\d{1,4})(?:v\d)?(?:\s|$)"));
static DASH_EPISODE: LazyLock<Regex> = LazyLock::new(|| re(r"\s[-–—]\s?(\d{1,4})(?:v\d)?(?:\s|$)"));
/// A trailing number of two or three digits, or with a leading zero (not a year).
static TRAILING_EPISODE: LazyLock<Regex> =
    LazyLock::new(|| re(r"(?:^|\s)(0\d{1,3}|[1-9]\d{1,2})(?:v\d)?$"));
static SEASON_IN_TITLE: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?i)(?:\s|^)(?:season\s?(\d{1,2})|s(\d{1,2})|(\d{1,2})(?:st|nd|rd|th)\s+season)$|第\s*(\d{1,2})\s*[期季]",
    )
});
static TRAILING_SEASON: LazyLock<Regex> = LazyLock::new(|| re(r"(?:^|\s)([2-9])$"));
/// A secondary title between tildes: `無職転生～到了异世界就拿出真本事～`.
static TILDE_SECONDARY: LazyLock<Regex> = LazyLock::new(|| re(r"[～〜~][^～〜~]+[～〜~]"));

fn number(s: &str) -> Option<u32> {
    s.parse().ok()
}

fn strip_extension(name: &str) -> &str {
    match name.rsplit_once('.') {
        Some((stem, ext)) if EXTENSIONS.contains(&ext.to_lowercase().as_str()) => stem,
        _ => name,
    }
}

fn squeeze(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The title with the separators at its ends removed.
fn trim_title(s: &str) -> String {
    squeeze(s)
        .trim_matches(|c: char| c.is_whitespace() || "-_.·~～〜|:".contains(c))
        .to_string()
}

/// Drops the technical tokens.
fn without_tech(s: &str) -> String {
    s.split_whitespace()
        .filter(|w| !TECH.is_match(w))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The season a title ends with (`Show Season 2`, `Show S2`, `Show 2nd Season`,
/// `Show 第2期`), and the title without it.
fn season_of_title(title: &str) -> (String, Option<u32>) {
    match SEASON_IN_TITLE.captures(title) {
        Some(c) => {
            let season = (1..=4)
                .find_map(|i| c.get(i))
                .and_then(|m| number(m.as_str()));
            let whole = c.get(0).map_or(0..0, |m| m.range());
            let rest = format!("{} {}", &title[..whole.start], &title[whole.end..]);
            (trim_title(&rest), season)
        }
        None => (title.to_string(), None),
    }
}

/// Parses a file name, with or without its extension.
pub fn parse_name(name: &str) -> NameInfo {
    let stem = strip_extension(name);
    let explicit = EXPLICIT
        .captures(stem)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string());
    let explicit_number = |re: &Regex| {
        explicit
            .as_deref()
            .and_then(|b| re.captures(b))
            .and_then(|c| number(&c[1]))
    };
    let mut text = EXPLICIT.replace_all(stem, " ").replace('\u{3000}', " ");
    if !text.contains(' ') {
        text = text.replace(['.', '_'], " ");
    } else {
        text = text.replace('_', " ");
    }
    // the blocks: a bare number may be the episode; the rest is dropped, but
    // a name made only of blocks keeps them as its text
    let mut blocks: Vec<String> = Vec::new();
    let outside = BRACKET
        .replace_all(&text, |c: &regex::Captures<'_>| {
            let inner = (1..=4).find_map(|i| c.get(i)).map_or("", |m| m.as_str());
            blocks.push(inner.trim().to_string());
            " "
        })
        .into_owned();
    // a year in a block is not an episode: `Movie (2016)`
    let block_episode = blocks.iter().find_map(|b| {
        BLOCK_EPISODE
            .captures(b)
            .and_then(|c| number(&c[1]))
            .filter(|n| !(1900..=2099).contains(n))
    });
    let mut text = without_tech(&outside);
    if text.trim().is_empty() {
        // `[Group][Title S3][01][1080P]`: the blocks but the first (the group)
        text = blocks
            .iter()
            .skip(1)
            .filter(|b| !BLOCK_EPISODE.is_match(b))
            .map(|b| without_tech(b))
            .filter(|b| !b.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
    }
    let text = squeeze(&text);

    let mut season = None;
    let mut episode = None;
    let mut title_end = text.len();
    let mut after_start = None;
    if let Some(c) = SEASON_EPISODE.captures(&text) {
        season = number(&c[1]);
        episode = number(&c[2]);
        let m = c.get(0).map_or(0..0, |m| m.range());
        title_end = m.start;
        after_start = Some(m.end);
    } else {
        for pattern in [&*CJK_EPISODE, &*EP_TOKEN, &*HASH_EPISODE, &*DASH_EPISODE] {
            if let Some(c) = pattern.captures(&text) {
                episode = number(&c[1]);
                let m = c.get(0).map_or(0..0, |m| m.range());
                title_end = m.start;
                after_start = Some(m.end);
                break;
            }
        }
        if episode.is_none()
            && let Some(c) = TRAILING_EPISODE.captures(&text)
        {
            episode = number(&c[1]);
            title_end = c.get(0).map_or(text.len(), |m| m.start());
        }
    }
    if episode.is_none() {
        episode = block_episode;
    }
    // a secondary title between tildes goes before the ends are trimmed
    let title_of = |raw: &str| trim_title(&TILDE_SECONDARY.replace_all(raw, " "));
    let mut title = title_of(&text[..title_end]);
    if title.is_empty()
        && let Some(start) = after_start
    {
        // `S01E04 - Title`
        title = title_of(&text[start..]);
    }
    let (title, title_season) = season_of_title(&title);
    let title = if title.is_empty() {
        trim_title(stem)
    } else {
        title
    };
    NameInfo {
        title,
        season: explicit_number(&EXPLICIT_SEASON)
            .or(season)
            .or(title_season),
        episode: explicit_number(&EXPLICIT_EPISODE).or(episode),
    }
}

/// The season a folder name gives: `Season 2`, `S02`, `第2季`, `2nd Season`,
/// or a trailing number from 2 to 9 (`Hibike! Euphonium 2`).
pub fn folder_season(folder: &str) -> Option<u32> {
    let folder = squeeze(&folder.replace(['.', '_'], " "));
    season_of_title(&folder).1.or_else(|| {
        TRAILING_SEASON
            .captures(&folder)
            .and_then(|c| number(&c[1]))
    })
}

/// Parses the file name of `path`; without a season in the name, an episode
/// takes the season of its folder.
pub fn parse_path(path: &Path) -> NameInfo {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut info = parse_name(&name);
    if info.season.is_none()
        && info.episode.is_some()
        && let Some(folder) = path.parent().and_then(Path::file_name)
    {
        info.season = folder_season(&folder.to_string_lossy());
    }
    info
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(name: &str) -> (String, Option<u32>, Option<u32>) {
        let i = parse_name(name);
        (i.title, i.season, i.episode)
    }

    fn t(
        title: &str,
        season: Option<u32>,
        episode: Option<u32>,
    ) -> (String, Option<u32>, Option<u32>) {
        (title.to_string(), season, episode)
    }

    // T-MET-010: Fushi `fushi/test/media/video/video_filename_parser_test.dart`
    #[test]
    fn fushi_vectors() {
        assert_eq!(
            parsed("[SubGroup] Title - 12 [1080p][x264].mkv"),
            t("Title", None, Some(12))
        );
        assert_eq!(
            parsed("[Kamigami] Himouto! Umaru-chan - 10 [1920x1080 x264 AAC Sub(Chs,Cht,Jap)].mkv"),
            t("Himouto! Umaru-chan", None, Some(10))
        );
        assert_eq!(
            parsed("Title.S02E05.1080p.WEB-DL.mkv"),
            t("Title", Some(2), Some(5))
        );
        assert_eq!(
            parsed("[Group] Series Name - S01E03 [x265].mkv"),
            t("Series Name", Some(1), Some(3))
        );
        assert_eq!(parsed("Show 第12話.mkv"), t("Show", None, Some(12)));
        assert_eq!(
            parsed("[ABC] 鬼滅の刃 - 08 [1080p][x264].mkv"),
            t("鬼滅の刃", None, Some(8))
        );
        assert_eq!(parsed("Show EP05.mp4"), t("Show", None, Some(5)));
        assert_eq!(parsed("My Anime 03.mp4"), t("My Anime", None, Some(3)));
        assert_eq!(
            parsed("Aria the Animation.mkv"),
            t("Aria the Animation", None, None)
        );
        assert_eq!(
            parsed("Cowboy.Bebop.第05話.mkv"),
            t("Cowboy Bebop", None, Some(5))
        );
        assert_eq!(
            parsed("Show {[tmdbid=777;type=tv;g=group-1;s=2;e=3]}.mkv"),
            t("Show", Some(2), Some(3))
        );
        assert_eq!(
            parsed(
                "[DBD-Raws][Re Zero kara Hajimeru Isekai Seikatsu S3][01][1080P][BDRip][HEVC-10bit][FLACx2].mkv"
            ),
            t("Re Zero kara Hajimeru Isekai Seikatsu", Some(3), Some(1))
        );
        assert_eq!(
            parsed("[桜都字幕组] 无职转生～到了异世界就拿出真本事～ [11][1080p][简繁内封].mkv"),
            t("无职转生", None, Some(11))
        );
        assert_eq!(
            parsed("日々は過ぎれど飯うまし.S01E09.出店してみますか!.WEBRip.Netflix.ja[cc].mkv"),
            t("日々は過ぎれど飯うまし", Some(1), Some(9))
        );
        assert_eq!(
            parsed("日々は過ぎれど飯うまし.S01E05.ドライブ行かない_.WEBRip.Netflix.ja[cc].mkv"),
            t("日々は過ぎれど飯うまし", Some(1), Some(5))
        );
        assert_eq!(parsed("S01E04 - Title.mkv"), t("Title", Some(1), Some(4)));
        let device = parse_name("VID_20260701.mp4");
        assert!(!device.title.is_empty());
        assert_eq!(device.episode, None);
        let obs = parse_name("2026-07-01 21-03-55.mkv");
        assert!(!obs.title.is_empty());
        assert_eq!(obs.episode, None);
    }

    // T-MET-010
    #[test]
    fn more_names() {
        assert_eq!(
            parsed("[SubsPlease] Sousou no Frieren - 01 (1080p) [ABCD1234].mkv"),
            t("Sousou no Frieren", None, Some(1))
        );
        assert_eq!(
            parsed("Kusuriya no Hitorigoto 2nd Season - 05.mkv"),
            t("Kusuriya no Hitorigoto", Some(2), Some(5))
        );
        assert_eq!(
            parsed("Show Season 3 - 07.mkv"),
            t("Show", Some(3), Some(7))
        );
        assert_eq!(
            parsed("進撃の巨人 第2期 #3.mp4"),
            t("進撃の巨人", Some(2), Some(3))
        );
        assert_eq!(parsed("Movie (2016).mkv"), t("Movie", None, None));
        assert_eq!(parsed("Show - 12v2 [720p].mkv"), t("Show", None, Some(12)));
        assert_eq!(parsed("Show.E07.ja.srt"), t("Show", None, Some(7)));
    }

    // T-MET-010
    #[test]
    fn the_folder_gives_the_season() {
        let info = parse_path(Path::new("/v/Show/Season 2/Show - 03.mkv"));
        assert_eq!((info.season, info.episode), (Some(2), Some(3)));
        let info = parse_path(Path::new("/v/Show/S02/Show - 03.mkv"));
        assert_eq!(info.season, Some(2));
        let info = parse_path(Path::new(
            "/v/Hibike! Euphonium 2/Hibike! Euphonium 2 - 04.mkv",
        ));
        assert_eq!(info.season, Some(2));
        // a film has no episode: the folder is not a season
        let info = parse_path(Path::new("/v/Ip Man 2/Ip Man 2.mkv"));
        assert_eq!(info.season, None);
        assert_eq!(folder_season("第3季"), Some(3));
        assert_eq!(folder_season("Specials"), None);
    }
}
