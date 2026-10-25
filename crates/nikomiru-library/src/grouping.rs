// SPDX-License-Identifier: GPL-3.0-only

//! The videos of an imported folder grouped into series (LIB-002, LIB-003):
//! by the title their names give, or by the folder they are in.
//!
//! Translated from Fushi `packages/fushi_engine/lib/media/video/video_filename_parser.dart`
//! (`groupVideosIntoPlaylists`, `resolveSiblingEpisodeNumbers`, commit
//! `ac8034e`), with its test vectors. One change: a file named only by its
//! episode in a season folder (`Show/Season 2/01.mkv`) belongs to the series
//! of the folder above, in that season; Fushi keeps `Season 2` as the series,
//! because its episodes were keyed by number and the seasons would collide.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use nikomiru_text::filename::{NameInfo, folder_season, parse_name};
use regex::Regex;

/// An episode of a group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Episode {
    /// The file.
    pub path: PathBuf,
    /// Its name without the extension.
    pub title: String,
    /// The season, from the name or the folder.
    pub season: Option<u32>,
    /// The episode, from the name or its siblings.
    pub episode: Option<u32>,
}

/// A series, or a folder: one episode or more, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    /// Its name.
    pub series: String,
    /// What identifies it again at the next scan: the series name in lower
    /// case, or the folder's path.
    pub key: String,
    /// Its episodes, by season, episode and name.
    pub episodes: Vec<Episode>,
}

fn re(pattern: &str) -> Regex {
    // the patterns are constants: a wrong one fails every test of this module
    Regex::new(pattern).unwrap_or_else(|e| panic!("bad pattern {pattern}: {e}"))
}

/// The episode left between the common ends of siblings: 1 to 3 digits (4
/// would be a year, `Movie (1979)`).
static EPISODE_DIGITS: LazyLock<Regex> = LazyLock::new(|| re(r"^\d{1,3}$"));
/// A season or episode marker the common beginning ends with (`Show S01E`).
static TRAILING_MARKER: LazyLock<Regex> =
    LazyLock::new(|| re(r"(?:^|[\s._-])(?:[Ss]\d{1,2}[\s._-]*)?(?:[Ee][Pp]?|#|第)$"));
static SPACES: LazyLock<Regex> = LazyLock::new(|| re(r"\s+"));
static LEADING_JUNK: LazyLock<Regex> = LazyLock::new(|| re(r"^[\s\-_.·\[(【]+"));
static TRAILING_JUNK: LazyLock<Regex> = LazyLock::new(|| re(r"[\s\-_.·\[(【]+$"));

fn stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn name(path: &Path) -> String {
    path.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// The episode numbers of files of one folder, read from what differs
/// between their names, and the series of their common beginning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Siblings {
    /// The number of each file, in the order given.
    pub numbers: Vec<u32>,
    /// The series their common beginning names; empty if none.
    pub series: String,
}

/// The episode numbers of `paths` when their names differ only by them
/// (`Show 1.mkv … Show 12.mkv`): at least two different names, the common
/// ends not cutting a number, 1 to 3 digits left in each, all different.
pub fn sibling_numbers(paths: &[PathBuf]) -> Option<Siblings> {
    if paths.len() < 2 {
        return None;
    }
    let stems: Vec<Vec<char>> = paths.iter().map(|p| stem(p).chars().collect()).collect();
    let mut unique = stems.clone();
    unique.sort();
    unique.dedup();
    if unique.len() != stems.len() {
        return None;
    }
    let first = &stems[0];
    let mut prefix = first.len();
    let mut suffix = first.len();
    for s in &stems[1..] {
        prefix = first
            .iter()
            .zip(s)
            .take(prefix)
            .take_while(|(a, b)| a == b)
            .count();
        suffix = first
            .iter()
            .rev()
            .zip(s.iter().rev())
            .take(suffix)
            .take_while(|(a, b)| a == b)
            .count();
    }
    // `Show 10/11/12` share `Show 1`: the ends go back to the whole number
    while prefix > 0 && first[prefix - 1].is_ascii_digit() {
        prefix -= 1;
    }
    while suffix > 0 && first[first.len() - suffix].is_ascii_digit() {
        suffix -= 1;
    }
    let mut numbers = Vec::new();
    for s in &stems {
        if prefix + suffix > s.len() {
            return None;
        }
        let middle: String = s[prefix..s.len() - suffix].iter().collect();
        if !EPISODE_DIGITS.is_match(&middle) {
            return None;
        }
        let n: u32 = middle.parse().ok()?;
        if numbers.contains(&n) {
            return None;
        }
        numbers.push(n);
    }
    let raw: String = first[..prefix].iter().collect();
    let parsed = parse_name(&raw);
    let series = strip_trailing_marker(if parsed.named { &parsed.title } else { &raw });
    Some(Siblings { numbers, series })
}

fn trim_edges(s: &str) -> String {
    let s = SPACES
        .replace_all(&s.replace('_', " "), " ")
        .trim()
        .to_string();
    let s = LEADING_JUNK.replace(&s, "").into_owned();
    TRAILING_JUNK.replace(&s, "").trim().to_string()
}

/// `Show S01E` → `Show`; `第` → empty.
fn strip_trailing_marker(s: &str) -> String {
    trim_edges(&TRAILING_MARKER.replace(&trim_edges(s), ""))
}

/// Compares names with their numbers as numbers: `9` before `100`.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    fn runs(s: &str) -> Vec<(bool, String)> {
        let mut out: Vec<(bool, String)> = Vec::new();
        for c in s.chars() {
            let digit = c.is_ascii_digit();
            match out.last_mut() {
                Some((d, run)) if *d == digit => run.push(c),
                _ => out.push((digit, c.to_string())),
            }
        }
        out
    }
    let (ra, rb) = (runs(a), runs(b));
    for ((da, a), (db, b)) in ra.iter().zip(&rb) {
        let order = if *da && *db {
            let (a, b) = (a.trim_start_matches('0'), b.trim_start_matches('0'));
            a.len().cmp(&b.len()).then_with(|| a.cmp(b))
        } else {
            a.to_lowercase().cmp(&b.to_lowercase())
        };
        if order != Ordering::Equal {
            return order;
        }
    }
    ra.len().cmp(&rb.len()).then_with(|| a.cmp(b))
}

fn compare(a: &Episode, b: &Episode) -> Ordering {
    a.season
        .unwrap_or(1)
        .cmp(&b.season.unwrap_or(1))
        .then_with(|| {
            a.episode
                .unwrap_or(u32::MAX)
                .cmp(&b.episode.unwrap_or(u32::MAX))
        })
        .then_with(|| natural_cmp(&a.title, &b.title))
}

/// What a file is: its name read, with the folder's season, the episode
/// from its siblings, and the series of the folder when its name has none.
struct Read {
    info: NameInfo,
    episode: Option<u32>,
    series: String,
}

/// Reads `paths`, completing the episodes from the siblings of each folder
/// that has a file without one, when they agree with those it has.
fn read_all(paths: &[PathBuf]) -> Vec<Read> {
    let mut reads: Vec<Read> = paths
        .iter()
        .map(|path| {
            let mut info = parse_name(&name(path));
            let parent = path.parent().and_then(Path::file_name);
            let parent_name = parent.map(|n| n.to_string_lossy().into_owned());
            let in_season_folder = parent_name.as_deref().and_then(folder_season);
            if info.season.is_none() && info.episode.is_some() {
                info.season = in_season_folder;
            }
            let series = series_of(path, &info, in_season_folder.is_some());
            Read {
                episode: info.episode,
                info,
                series,
            }
        })
        .collect();
    let mut by_folder: BTreeMap<PathBuf, Vec<usize>> = BTreeMap::new();
    for (i, path) in paths.iter().enumerate() {
        let folder = path.parent().map(Path::to_path_buf).unwrap_or_default();
        by_folder.entry(folder).or_default().push(i);
    }
    for indices in by_folder.values() {
        if indices.len() < 2 || indices.iter().all(|&i| reads[i].episode.is_some()) {
            continue;
        }
        let folder: Vec<PathBuf> = indices.iter().map(|&i| paths[i].clone()).collect();
        let Some(sib) = sibling_numbers(&folder) else {
            continue;
        };
        let agrees = indices
            .iter()
            .zip(&sib.numbers)
            .all(|(&i, n)| reads[i].episode.is_none_or(|e| e == *n));
        if !agrees {
            continue;
        }
        for (&i, n) in indices.iter().zip(&sib.numbers) {
            reads[i].episode.get_or_insert(*n);
            if !sib.series.is_empty() {
                reads[i].series.clone_from(&sib.series);
            }
        }
    }
    reads
}

/// The series of a file: the title of its name; for a name that is only an
/// episode, the title of its folder (or of the folder above a season
/// folder), else the folder's own name.
fn series_of(path: &Path, info: &NameInfo, in_season_folder: bool) -> String {
    if info.episode.is_none() || info.named {
        return info.title.clone();
    }
    let mut folder = path.parent();
    if in_season_folder {
        folder = folder.and_then(Path::parent);
    }
    let Some(folder) = folder.and_then(Path::file_name) else {
        return info.title.clone();
    };
    let folder = folder.to_string_lossy();
    let parsed = parse_name(&folder);
    if parsed.named {
        parsed.title
    } else if folder.trim().is_empty() {
        info.title.clone()
    } else {
        folder.trim().to_string()
    }
}

fn episode(path: &Path, read: &Read) -> Episode {
    Episode {
        path: path.to_path_buf(),
        title: stem(path),
        season: read.info.season,
        episode: read.episode,
    }
}

fn finish(mut groups: Vec<Group>) -> Vec<Group> {
    for g in &mut groups {
        g.episodes.sort_by(compare);
    }
    groups.sort_by(|a, b| natural_cmp(&a.series, &b.series).then_with(|| a.key.cmp(&b.key)));
    groups
}

/// LIB-003: the videos grouped by the series their names give.
pub fn by_series(paths: &[PathBuf]) -> Vec<Group> {
    let reads = read_all(paths);
    let mut groups: Vec<Group> = Vec::new();
    for (path, read) in paths.iter().zip(&reads) {
        let key = read.series.to_lowercase();
        let e = episode(path, read);
        match groups.iter_mut().find(|g| g.key == key) {
            Some(g) => g.episodes.push(e),
            None => groups.push(Group {
                series: read.series.clone(),
                key,
                episodes: vec![e],
            }),
        }
    }
    finish(groups)
}

/// LIB-002: the videos grouped by the folder they are in, each named after
/// it (`root`'s own name for the files at its top).
pub fn by_folder(paths: &[PathBuf]) -> Vec<Group> {
    let reads = read_all(paths);
    let mut groups: Vec<Group> = Vec::new();
    for (path, read) in paths.iter().zip(&reads) {
        let folder = path.parent().unwrap_or(Path::new(""));
        let key = folder.to_string_lossy().into_owned();
        let e = episode(path, read);
        match groups.iter_mut().find(|g| g.key == key) {
            Some(g) => g.episodes.push(e),
            None => groups.push(Group {
                series: folder
                    .file_name()
                    .map_or_else(|| key.clone(), |n| n.to_string_lossy().into_owned()),
                key,
                episodes: vec![e],
            }),
        }
    }
    finish(groups)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(names: &[&str]) -> Vec<PathBuf> {
        names.iter().map(PathBuf::from).collect()
    }

    fn episodes(g: &Group) -> Vec<Option<u32>> {
        g.episodes.iter().map(|e| e.episode).collect()
    }

    fn series(groups: &[Group]) -> Vec<&str> {
        groups.iter().map(|g| g.series.as_str()).collect()
    }

    // T-LIB-003: Fushi `groupVideosIntoPlaylists`
    #[test]
    fn a_series_is_one_group_in_order() {
        let g = by_series(&paths(&[
            "/v/[G] Title - 03 [1080p].mkv",
            "/v/[G] Title - 01 [1080p].mkv",
            "/v/[G] Title - 02 [1080p].mkv",
        ]));
        assert_eq!(series(&g), ["Title"]);
        assert_eq!(episodes(&g[0]), [Some(1), Some(2), Some(3)]);

        let g = by_series(&paths(&[
            "/v/Beta - 01.mkv",
            "/v/Alpha - 02.mkv",
            "/v/Alpha - 01.mkv",
        ]));
        assert_eq!(series(&g), ["Alpha", "Beta"]);
        assert_eq!((g[0].episodes.len(), g[1].episodes.len()), (2, 1));

        let g = by_series(&paths(&[
            "/v/Show.S02E01.mkv",
            "/v/Show.S01E02.mkv",
            "/v/Show.S01E01.mkv",
        ]));
        let order: Vec<(Option<u32>, Option<u32>)> = g[0]
            .episodes
            .iter()
            .map(|e| (e.season, e.episode))
            .collect();
        assert_eq!(
            order,
            [(Some(1), Some(1)), (Some(1), Some(2)), (Some(2), Some(1))]
        );

        let g = by_series(&paths(&["/v/Standalone Movie.mkv"]));
        assert_eq!(g[0].episodes[0].title, "Standalone Movie");
        assert!(by_series(&[]).is_empty());
    }

    // T-LIB-003: episode titles that differ (Fushi BUG-1435)
    #[test]
    fn episode_titles_do_not_split_a_season() {
        let prefix = "/v/日々は過ぎれど飯うまし.S01E";
        let suffix = ".WEBRip.Netflix.ja[cc].mkv";
        let names: Vec<String> = [
            "09.出店してみますか!",
            "10.ただいま",
            "07.ずっと忘れないと思う",
            "12.ごちそうさま!!",
            "04.この子は星なな",
            "03.お金なくなっちゃった!!",
            "05.ドライブ行かない_",
            "11.クリスマス空いてますか!_",
        ]
        .iter()
        .map(|n| format!("{prefix}{n}{suffix}"))
        .collect();
        let g = by_series(&names.iter().map(PathBuf::from).collect::<Vec<_>>());
        assert_eq!(series(&g), ["日々は過ぎれど飯うまし"]);
        assert_eq!(episodes(&g[0]), [3, 4, 5, 7, 9, 10, 11, 12].map(Some));
    }

    // T-LIB-003: a name that is only the episode takes its folder's
    #[test]
    fn the_folder_names_a_series_of_bare_episodes() {
        let g = by_series(&paths(&[
            "/Videos/動漫/葬送的芙莉莲/02.mp4",
            "/Videos/動漫/葬送的芙莉莲/01.mp4",
            "/Videos/動漫/葬送的芙莉莲/13.mp4",
        ]));
        assert_eq!(series(&g), ["葬送的芙莉莲"]);
        assert_eq!(episodes(&g[0]), [Some(1), Some(2), Some(13)]);
        assert_eq!(
            series(&by_series(&paths(&[
                "/v/间谍过家家/第01集.mp4",
                "/v/间谍过家家/第02集.mp4"
            ]))),
            ["间谍过家家"]
        );
        assert_eq!(
            series(&by_series(&paths(&[
                "/v/Dandadan/S01E01.mkv",
                "/v/Dandadan/S01E02.mkv"
            ]))),
            ["Dandadan"]
        );
        // a name with a title does not look at the folder
        assert_eq!(
            series(&by_series(&paths(&[
                "/v/合集目录/Alpha - 01.mkv",
                "/v/合集目录/Beta - 01.mkv"
            ]))),
            ["Alpha", "Beta"]
        );
        assert_eq!(
            series(&by_series(&paths(&["/v/杂/01.mkv", "/v/杂/02.mkv"]))),
            ["杂"]
        );
        assert_eq!(series(&by_series(&paths(&["01.mkv"]))), ["01"]);
    }

    // T-LIB-003: unlike Fushi, season folders make one series
    #[test]
    fn season_folders_are_seasons_of_one_series() {
        let g = by_series(&paths(&[
            "/v/Show/Season 1/01.mkv",
            "/v/Show/Season 2/01.mkv",
        ]));
        assert_eq!(series(&g), ["Show"]);
        let order: Vec<(Option<u32>, Option<u32>)> = g[0]
            .episodes
            .iter()
            .map(|e| (e.season, e.episode))
            .collect();
        assert_eq!(order, [(Some(1), Some(1)), (Some(2), Some(1))]);
        // with the title in the name too
        let g = by_series(&paths(&[
            "/v/Show/Season 2/Show - 03.mkv",
            "/v/Show/Season 1/Show - 03.mkv",
        ]));
        assert_eq!(g[0].episodes[0].season, Some(1));
    }

    // T-LIB-003: Fushi BUG-2369, the siblings number the episodes
    #[test]
    fn siblings_number_unpadded_episodes() {
        let names: Vec<PathBuf> = (1..=12)
            .map(|i| PathBuf::from(format!("/anime/Chuunibyou/Chuunibyou {i}.mkv")))
            .collect();
        let g = by_series(&names);
        assert_eq!(series(&g), ["Chuunibyou"]);
        assert_eq!(episodes(&g[0]), (1..=12).map(Some).collect::<Vec<_>>());

        let names: Vec<PathBuf> = (1..=12)
            .map(|i| PathBuf::from(format!("/anime/Show/Show!{i}.mkv")))
            .collect();
        assert_eq!(
            episodes(&by_series(&names)[0]),
            (1..=12).map(Some).collect::<Vec<_>>()
        );

        let sib = sibling_numbers(&paths(&[
            "/a/Show 10.mkv",
            "/a/Show 11.mkv",
            "/a/Show 12.mkv",
        ]))
        .unwrap();
        assert_eq!(
            (sib.numbers, sib.series.as_str()),
            (vec![10, 11, 12], "Show")
        );

        // the numbers found are only completed, never overwritten
        let g = by_series(&paths(&[
            "/a/Show - Interview 1 [BD].mkv",
            "/a/Show - Interview 2 [BD].mkv",
            "/a/Show - Interview 10 [BD].mkv",
        ]));
        assert_eq!(episodes(&g[0]), [Some(1), Some(2), Some(10)]);

        // folders apart
        let g = by_series(&paths(&[
            "/a/S1/Show 1.mkv",
            "/a/S1/Show 2.mkv",
            "/a/X/Show 10.mkv",
            "/a/X/Show 11.mkv",
        ]));
        let all: Vec<Option<u32>> = g.iter().flat_map(episodes).collect();
        assert_eq!(all, [Some(1), Some(2), Some(10), Some(11)]);
    }

    // T-LIB-003: when the siblings do not settle it, nothing is guessed
    #[test]
    fn siblings_that_disagree_give_nothing() {
        assert!(
            sibling_numbers(&paths(&[
                "/a/Show 1.mkv",
                "/a/Show 2.mkv",
                "/a/Show OVA.mkv"
            ]))
            .is_none()
        );
        assert!(sibling_numbers(&paths(&["/a/Movie (1979).mkv", "/a/Movie (2005).mkv"])).is_none());
        assert!(sibling_numbers(&paths(&["/a/01.mkv", "/b/01.mkv"])).is_none());
        assert!(sibling_numbers(&paths(&["/a/Show 1.mkv"])).is_none());
        // a number already read agrees: the others are completed
        let g = by_series(&paths(&[
            "/a/Show 1.mkv",
            "/a/Show 2.mkv",
            "/a/Show 07.mkv",
        ]));
        assert_eq!(episodes(&g[0]), [Some(1), Some(2), Some(7)]);
        // no episode in any name: the natural order of the names
        assert!(g_none(&["/a/Show - Talk 9 extra (x).mkv"]));
        let g = by_series(&paths(&[
            "/a/Show - Talk 100 extra (x).mkv",
            "/a/Show - Talk extra (x).mkv",
            "/a/Show - Talk 9 extra (x).mkv",
        ]));
        let titles: Vec<&str> = g
            .iter()
            .flat_map(|g| g.episodes.iter().map(|e| e.title.as_str()))
            .collect();
        assert_eq!(
            titles,
            [
                "Show - Talk 9 extra (x)",
                "Show - Talk 100 extra (x)",
                "Show - Talk extra (x)"
            ]
        );
    }

    fn g_none(names: &[&str]) -> bool {
        by_series(&paths(names))
            .iter()
            .all(|g| g.episodes.iter().all(|e| e.episode.is_none()))
    }

    // T-LIB-002: by folder
    #[test]
    fn by_folder_each_folder_is_a_group() {
        let g = by_folder(&paths(&[
            "/lib/Alpha - 01.mkv",
            "/lib/Extras/Beta - 01.mkv",
            "/lib/Extras/Alpha - 02.mkv",
        ]));
        assert_eq!(series(&g), ["Extras", "lib"]);
        assert_eq!(g[0].key, "/lib/Extras");
        assert_eq!(g[0].episodes.len(), 2);
        assert_eq!(natural_cmp("Ep 9", "Ep 10"), Ordering::Less);
        assert_eq!(natural_cmp("b2", "B10"), Ordering::Less);
    }
}
