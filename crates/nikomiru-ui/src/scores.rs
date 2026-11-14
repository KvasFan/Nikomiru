// SPDX-License-Identifier: GPL-3.0-only

//! The comprehension scores of a series' episodes before they play
//! (KNW-008): for each episode, the subtitle file the player would load by
//! itself (SUB-006, SUB-008, MET-008), its lines split into words away from
//! the UI thread once, and the score worked out again from those words
//! whenever the statuses change. The app's counter also probes and extracts
//! local embedded text tracks (2.30.0), using the same selection policy.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use nikomiru_dict::segment::Word;
use nikomiru_library::SubtitleChoice;
use nikomiru_library::words::{Counted, Score};
use nikomiru_subs::select::{self, Choice, External};

/// The files of `dir`.
fn files(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
                .map(|e| e.path())
                .collect()
        })
        .unwrap_or_default()
}

/// The subtitle files of `video` (SUB-006): those with its name next to it
/// or in the folders releases keep subtitles in; else those of its episode,
/// or of a folder of its own; and the ones `downloaded` for it (MET-008).
pub fn subtitle_files(video: &Path, downloaded: &[PathBuf]) -> Vec<External> {
    let folder = video.parent().map(Path::to_path_buf).unwrap_or_default();
    let mut entries = files(&folder);
    // the folders releases keep subtitles in (SUB-006), and the one named
    // after this video inside them (`Subs/<video name>/`)
    let stem = video
        .file_stem()
        .map(|s| s.to_os_string())
        .unwrap_or_default();
    let mut own = Vec::new();
    for name in select::SUBTITLE_FOLDERS {
        let sub = folder.join(name);
        if sub.is_dir() {
            entries.extend(files(&sub));
            own.extend(files(&sub.join(&stem)));
        }
    }
    let mut externals = select::external_files(video, &entries);
    if externals.is_empty() {
        // none with its name: those of its episode, or of a folder of its own
        let only_video = !entries.iter().any(|p| {
            p != video
                && p.parent() == video.parent()
                && p.extension().is_some_and(|e| {
                    let e = format!("*.{}", e.to_string_lossy().to_lowercase());
                    crate::view::VIDEO_PATTERNS.contains(&e.as_str())
                })
        });
        let language =
            |name: &str| nikomiru_online::language::subtitle_language(name).map(str::to_string);
        let read = select::NameReader {
            episode: &nikomiru_online::language::subtitle_episode,
            language: &language,
        };
        externals = select::related_files(video, &entries, &own, only_video, &read);
    }
    externals.extend(select::downloaded_files(downloaded));
    externals
}

/// A file whose selection needs no embedded metadata (SUB-008): a valid
/// saved file, or the first language's tagged/untagged file. Other cases
/// need a probe before trying the lower-priority languages. Off stays off.
pub fn decisive_file(
    externals: &[External],
    saved: Option<&SubtitleChoice>,
    preferred: &[String],
) -> Option<PathBuf> {
    let saved = match saved {
        Some(SubtitleChoice::File(p)) => Some(Choice::External(PathBuf::from(p))),
        Some(SubtitleChoice::Off | SubtitleChoice::Embedded { .. }) => return None,
        None => None,
    };
    preferred
        .first()
        .and_then(|lang| select::choose(saved.as_ref(), externals, &[], lang))
        .and_then(|c| match c {
            Choice::External(p) => Some(p),
            Choice::Embedded(_) => None,
        })
}

/// What to count for an episode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CountRequest {
    /// The video.
    pub video: PathBuf,
    /// The subtitles chosen for it or its series (LIB-010).
    pub saved: Option<SubtitleChoice>,
    /// `[subtitles] preferred_languages`.
    pub preferred: Vec<String>,
    /// Where the downloaded subtitles are kept (MET-008).
    pub subtitles_dir: PathBuf,
    /// The configured FFmpeg location, for embedded tracks (EXT-001).
    pub ffmpeg: String,
    /// The words' generation: a count made with other dictionaries is not
    /// reused.
    pub generation: u64,
    /// The UI count epoch: old replies cannot replace a newer request.
    pub epoch: u64,
}

/// The words of a subtitle file's lines: `split` gives the words of a line,
/// each text split once.
pub fn count(
    lines: impl IntoIterator<Item = String>,
    mut split: impl FnMut(&str) -> Result<Vec<Word>, String>,
) -> Result<Counted, String> {
    let mut known: HashMap<String, Vec<(String, String)>> = HashMap::new();
    let mut out = Vec::new();
    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        if !known.contains_key(&line) {
            let words = split(&line)?
                .into_iter()
                .filter_map(|w| w.headword)
                .map(|h| (h.expression, h.reading))
                .collect();
            known.insert(line.clone(), words);
        }
        out.push(known[&line].clone());
    }
    Ok(Counted::from_lines(out))
}

/// Where an episode's score is.
#[derive(Debug, Clone)]
enum Entry {
    Counting,
    /// No subtitle file to count, or it could not be read.
    None,
    Counted {
        counted: Arc<Counted>,
        /// The score, with the words' revision it was worked out for.
        score: Option<(u64, Score)>,
    },
}

/// What the episode list says of an episode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EpisodeScore {
    /// Its words are being counted.
    Counting,
    /// Its score.
    Score(Score),
}

/// The scores of the episodes of the series open, by uid.
#[derive(Debug, Default)]
pub struct EpisodeScores {
    /// The series they are of.
    series: Option<u64>,
    /// The words' generation they were counted with.
    generation: u64,
    /// UI count epoch, distinct from the dictionaries' cache generation.
    epoch: u64,
    preferred: Vec<String>,
    ffmpeg: String,
    entries: HashMap<String, Entry>,
}

impl EpisodeScores {
    /// Starts again for another series, dictionaries, languages or tool setting;
    /// returns whether it did.
    pub fn reset_for(
        &mut self,
        series: Option<u64>,
        generation: u64,
        preferred: &[String],
        ffmpeg: &str,
    ) -> bool {
        if (
            self.series,
            self.generation,
            self.preferred.as_slice(),
            self.ffmpeg.as_str(),
        ) == (series, generation, preferred, ffmpeg)
        {
            return false;
        }
        self.series = series;
        self.generation = generation;
        self.preferred = preferred.to_vec();
        self.ffmpeg = ffmpeg.into();
        self.invalidate();
        true
    }

    /// Reopening a series rechecks sources even when its id has not changed.
    pub fn invalidate(&mut self) {
        self.epoch += 1;
        self.entries.clear();
    }

    /// Stamp replies so older series/settings requests cannot replace them.
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// Whether `uid` was asked for.
    pub fn asked(&self, uid: &str) -> bool {
        self.entries.contains_key(uid)
    }

    /// `uid` is being counted.
    pub fn counting(&mut self, uid: String) {
        self.entries.insert(uid, Entry::Counting);
    }

    /// The count of `uid`, unless it belongs to an older count epoch.
    pub fn done(&mut self, epoch: u64, uid: String, result: Result<Option<Counted>, String>) {
        if epoch != self.epoch || !self.entries.contains_key(&uid) {
            return;
        }
        let entry = match result {
            Ok(Some(counted)) => Entry::Counted {
                counted: Arc::new(counted),
                score: None,
            },
            Ok(None) => Entry::None,
            Err(e) => {
                tracing::warn!("counting the words of an episode: {e}");
                Entry::None
            }
        };
        self.entries.insert(uid, entry);
    }

    /// Works the scores out again for the words' `revision`, with `status`
    /// saying what the learner knows of a word.
    pub fn update(
        &mut self,
        revision: u64,
        status: impl Fn(&str, &str) -> nikomiru_library::words::WordStatus,
    ) {
        for entry in self.entries.values_mut() {
            if let Entry::Counted { counted, score } = entry
                && score.is_none_or(|(r, _)| r != revision)
            {
                *score = Some((revision, counted.score(&status)));
            }
        }
    }

    /// A complete retained count for explicit pre-playback vocabulary browsing.
    pub fn counted(&self, uid: &str) -> Option<Arc<Counted>> {
        match self.entries.get(uid)? {
            Entry::Counted { counted, .. } => Some(Arc::clone(counted)),
            _ => None,
        }
    }

    /// What the episode list says of `uid`.
    pub fn get(&self, uid: &str) -> Option<EpisodeScore> {
        match self.entries.get(uid)? {
            Entry::Counting => Some(EpisodeScore::Counting),
            Entry::None => None,
            Entry::Counted { score, .. } => score.map(|(_, s)| EpisodeScore::Score(s)),
        }
    }
}

#[cfg(test)]
mod tests {
    use nikomiru_dict::segment::Headword;
    use nikomiru_library::words::WordStatus;

    use super::*;

    // T-KNW-031: only complete counts are browsable and source epochs discard them.
    #[test]
    fn vocabulary_access_requires_a_current_complete_count() {
        let mut scores = EpisodeScores::default();
        scores.reset_for(Some(1), 0, &[], "");
        scores.counting("ep".into());
        assert!(scores.counted("ep").is_none());
        let old = scores.epoch();
        let counted = Counted::from_lines([vec![("猫".into(), "ねこ".into()); 2]]);
        scores.done(old, "ep".into(), Ok(Some(counted.clone())));
        assert_eq!(*scores.counted("ep").unwrap(), counted);
        scores.invalidate();
        scores.done(old, "ep".into(), Ok(Some(counted)));
        assert!(scores.counted("ep").is_none());
    }

    fn external(path: &str, language: Option<&str>) -> External {
        External {
            path: PathBuf::from(path),
            format: nikomiru_subs::Format::Srt,
            language: language.map(str::to_string),
        }
    }

    // T-KNW-008: the file the player would load by itself
    #[test]
    fn the_file_counted_is_the_one_the_player_would_load() {
        let files = [
            external("/v/ep.en.srt", Some("en")),
            external("/v/ep.ja.srt", Some("ja")),
        ];
        let ja = ["ja".to_string()];
        assert_eq!(
            decisive_file(&files, None, &ja),
            Some("/v/ep.ja.srt".into())
        );
        let chosen = SubtitleChoice::File("/v/ep.en.srt".into());
        assert_eq!(
            decisive_file(&files, Some(&chosen), &ja),
            Some("/v/ep.en.srt".into())
        );
        let gone = SubtitleChoice::File("/v/other.srt".into());
        assert_eq!(
            decisive_file(&files, Some(&gone), &ja),
            Some("/v/ep.ja.srt".into())
        );
        assert_eq!(decisive_file(&files, Some(&SubtitleChoice::Off), &ja), None);
        let track = SubtitleChoice::Embedded {
            ff_index: Some(2),
            language: Some("jpn".into()),
            title: None,
        };
        assert_eq!(decisive_file(&files, Some(&track), &ja), None);
        assert_eq!(decisive_file(&files[..1], None, &ja), None, "English only");
        assert_eq!(
            decisive_file(&files[..1], None, &["ja".into(), "en".into()]),
            None,
            "must inspect Japanese embedded tracks first"
        );
        let plain = [external("/v/ep.srt", None)];
        assert_eq!(decisive_file(&plain, None, &ja), Some("/v/ep.srt".into()));
    }

    // T-KNW-008: the files next to a video and the ones downloaded
    #[test]
    fn the_files_of_a_video_are_found() {
        let dir = tempfile::tempdir().unwrap();
        let video = dir.path().join("Show 01.mkv");
        for name in [
            "Show 01.mkv",
            "Show 01.ja.srt",
            "Show 02.mkv",
            "Show 02.ja.srt",
        ] {
            std::fs::write(dir.path().join(name), "").unwrap();
        }
        let downloaded = dir.path().join("dl.ja.srt");
        let files = subtitle_files(&video, std::slice::from_ref(&downloaded));
        let paths: Vec<_> = files.iter().map(|e| e.path.clone()).collect();
        assert_eq!(paths, [dir.path().join("Show 01.ja.srt"), downloaded]);
    }

    fn word(e: &str, r: &str) -> Word {
        Word {
            range: 0..0,
            headword: Some(Headword {
                expression: e.into(),
                reading: r.into(),
                pitch: None,
            }),
        }
    }

    // T-KNW-008: a line said twice is split once
    #[test]
    fn each_text_is_split_once() {
        let mut asked = Vec::new();
        let counted = count(
            ["猫だ", "", "猫だ", "♪"].map(String::from),
            |text: &str| {
                asked.push(text.to_string());
                Ok(match text {
                    "猫だ" => vec![word("猫", "ねこ"), word("だ", "だ")],
                    _ => vec![Word {
                        range: 0..3,
                        headword: None,
                    }],
                })
            },
        )
        .unwrap();
        assert_eq!(asked, ["猫だ", "♪"]);
        assert_eq!(counted.lines, [vec![0, 1], vec![0, 1]]);
        assert!(count(["猫".to_string()], |_| Err("no dictionary".into())).is_err());
    }

    // T-KNW-008: counted once, worked out again when the statuses change;
    // a count for other dictionaries or another series dropped
    #[test]
    fn the_scores_follow_the_statuses() {
        let mut scores = EpisodeScores::default();
        assert!(!scores.reset_for(None, 0, &[], ""));
        assert!(scores.reset_for(Some(1), 0, &[], ""));
        scores.counting("a".into());
        scores.counting("b".into());
        assert_eq!(scores.get("a"), Some(EpisodeScore::Counting));
        let counted = Counted::from_lines([vec![("猫".to_string(), "ねこ".to_string())]]);
        scores.done(scores.epoch(), "a".into(), Ok(Some(counted)));
        scores.done(scores.epoch(), "b".into(), Ok(None));
        scores.done(scores.epoch(), "c".into(), Ok(None));
        assert!(!scores.asked("c"), "not asked");
        assert_eq!(scores.get("a"), None, "not worked out yet");
        scores.update(1, |_, _| WordStatus::Unknown);
        let Some(EpisodeScore::Score(s)) = scores.get("a") else {
            panic!()
        };
        assert_eq!((s.known_percent(), s.clear_percent()), (Some(0), Some(0)));
        scores.update(1, |_, _| WordStatus::Known);
        assert_eq!(
            scores.get("a"),
            Some(EpisodeScore::Score(s)),
            "same revision"
        );
        scores.update(2, |_, _| WordStatus::Known);
        let Some(EpisodeScore::Score(s)) = scores.get("a") else {
            panic!()
        };
        assert_eq!(s.known_percent(), Some(100));
        assert_eq!(scores.get("b"), None);
        // the dictionaries changed: an older count is dropped
        assert!(scores.reset_for(Some(1), 1, &[], ""));
        scores.counting("a".into());
        scores.done(0, "a".into(), Ok(None));
        assert_eq!(scores.get("a"), Some(EpisodeScore::Counting));
    }

    // T-KNW-008: a reply from the same dictionaries but older settings or
    // an earlier visit cannot replace the current series' scores.
    #[test]
    fn old_count_epochs_are_rejected_after_settings_or_a_reopen() {
        let mut scores = EpisodeScores::default();
        scores.reset_for(Some(1), 0, &["ja".into()], "");
        let old = scores.epoch();
        scores.counting("a".into());
        assert!(scores.reset_for(Some(1), 0, &["en".into()], ""));
        scores.counting("a".into());
        scores.done(old, "a".into(), Ok(None));
        assert_eq!(scores.get("a"), Some(EpisodeScore::Counting));
        let old = scores.epoch();
        assert!(scores.reset_for(Some(1), 0, &["en".into()], "/tools"));
        scores.counting("a".into());
        scores.done(old, "a".into(), Ok(None));
        assert_eq!(scores.get("a"), Some(EpisodeScore::Counting));
        let old = scores.epoch();
        scores.invalidate();
        scores.counting("a".into());
        scores.done(old, "a".into(), Ok(None));
        assert_eq!(scores.get("a"), Some(EpisodeScore::Counting));
        scores.done(scores.epoch(), "a".into(), Ok(None));
        assert_eq!(scores.get("a"), None);
    }
}
