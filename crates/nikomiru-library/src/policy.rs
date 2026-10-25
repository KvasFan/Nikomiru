// SPDX-License-Identifier: GPL-3.0-only

//! Where an episode starts (LIB-007), when it counts as watched (LIB-008),
//! the episode after it (LIB-009) and what it inherits from its series
//! (LIB-010).
//!
//! The start rule is Fushi's `fushi/lib/src/media/video/video_episode_start_policy.dart`
//! (commit `ac8034e`).

use crate::db::{Library, LibraryError, SubtitleChoice, VideoRecord};

/// The share of an episode from which it is watched, and from which it
/// starts again from the beginning (LIB-007, LIB-008), in percent.
pub const WATCHED_PERCENT: u64 = 90;

/// The time left under which an episode starts again from the beginning.
pub const END_MS: u64 = 3_000;

/// Why an episode is opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opening {
    /// Chosen by the user: it resumes.
    Chosen,
    /// "Previous episode", or the next one by itself: from the beginning.
    FromStart,
}

/// Where `v` starts, in ms (LIB-007): from 0 when asked, or when its saved
/// position is at 90% or less than 3 s from the end; else where it was left.
pub fn start_ms(v: &VideoRecord, opening: Opening) -> u64 {
    let at = v.last_position_ms;
    if opening == Opening::FromStart || at == 0 {
        return 0;
    }
    match v.duration_ms {
        Some(d) if at * 100 >= d * WATCHED_PERCENT || d.saturating_sub(at) < END_MS => 0,
        _ => at,
    }
}

/// Marks `v` watched the first time its position reaches 90% of its
/// duration (LIB-008); never again after.
pub fn mark_watched(v: &mut VideoRecord, at_ms: i64) {
    if v.completed_at.is_none()
        && let Some(d) = v.duration_ms.filter(|d| *d > 0)
        && v.last_position_ms * 100 >= d * WATCHED_PERCENT
    {
        v.completed_at = Some(at_ms);
    }
}

/// What an episode plays with (LIB-010): its own choices, else its series'.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Effective {
    /// The audio track's language, from the series (an episode keeps its own
    /// track id, `aid`).
    pub audio_language: Option<String>,
    /// The main subtitle.
    pub subtitle: Option<SubtitleChoice>,
    /// The subtitle offset.
    pub delay_ms: Option<i64>,
}

impl Library {
    /// The episode after `uid` in its series (LIB-009).
    pub fn next_episode(&self, uid: &str) -> Result<Option<(String, VideoRecord)>, LibraryError> {
        self.neighbour(uid, 1)
    }

    /// The episode before `uid` in its series.
    pub fn previous_episode(
        &self,
        uid: &str,
    ) -> Result<Option<(String, VideoRecord)>, LibraryError> {
        self.neighbour(uid, -1)
    }

    fn neighbour(
        &self,
        uid: &str,
        step: isize,
    ) -> Result<Option<(String, VideoRecord)>, LibraryError> {
        let Some(series) = self.video(uid)?.and_then(|v| v.series) else {
            return Ok(None);
        };
        let episodes = self.episodes(series)?;
        let Some(i) = episodes.iter().position(|(u, _)| u == uid) else {
            return Ok(None);
        };
        Ok(i.checked_add_signed(step)
            .and_then(|j| episodes.get(j))
            .cloned())
    }

    /// What `uid` plays with (LIB-010).
    pub fn effective(&self, uid: &str) -> Result<Effective, LibraryError> {
        let Some(v) = self.video(uid)? else {
            return Ok(Effective::default());
        };
        let prefs = match v.series {
            Some(id) => self.series(id)?.map(|s| s.prefs).unwrap_or_default(),
            None => Default::default(),
        };
        Ok(Effective {
            audio_language: prefs.audio_language,
            subtitle: v.subtitle.or(prefs.subtitle),
            delay_ms: v.delay_ms.or(prefs.delay_ms),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{Grouping, SeriesPrefs};
    use crate::scan::{Scanned, under};
    use std::path::Path;

    fn at(position: u64, duration: Option<u64>) -> VideoRecord {
        VideoRecord {
            last_position_ms: position,
            duration_ms: duration,
            ..VideoRecord::default()
        }
    }

    // T-LIB-007: Fushi `video_episode_start_policy_test.dart`
    #[test]
    fn where_an_episode_starts() {
        let d = Some(100_000);
        assert_eq!(start_ms(&at(90_000, d), Opening::Chosen), 0, "90%");
        assert_eq!(start_ms(&at(97_100, d), Opening::Chosen), 0, "2.9 s left");
        assert_eq!(
            start_ms(&at(50_000, d), Opening::FromStart),
            0,
            "previous or next"
        );
        assert_eq!(start_ms(&at(50_000, d), Opening::Chosen), 50_000);
        assert_eq!(start_ms(&at(89_999, d), Opening::Chosen), 89_999);
        // no duration yet: the position
        assert_eq!(start_ms(&at(50_000, None), Opening::Chosen), 50_000);
    }

    // T-LIB-008
    #[test]
    fn watched_is_set_once() {
        let mut v = at(80_000, Some(100_000));
        mark_watched(&mut v, 1);
        assert_eq!(v.completed_at, None);
        v.last_position_ms = 90_000;
        mark_watched(&mut v, 2);
        v.last_position_ms = 99_000;
        mark_watched(&mut v, 3);
        assert_eq!(v.completed_at, Some(2), "never overwritten");
        let mut none = at(90_000, None);
        mark_watched(&mut none, 4);
        assert_eq!(none.completed_at, None);
    }

    // T-LIB-009, T-LIB-010
    #[test]
    fn the_next_episode_and_the_series_choices() {
        let dir = tempfile::tempdir().unwrap();
        let lib = Library::open(&dir.path().join("library.redb")).unwrap();
        let root = Path::new("/v");
        let files: Vec<Scanned> = under(root, &["Show - 02.mkv", "Show - 01.mkv", "Show - 03.mkv"])
            .into_iter()
            .map(|path| Scanned {
                path,
                size: 1,
                mtime: 1,
            })
            .collect();
        lib.import(root, Grouping::BySeries, &files, 1).unwrap();
        let next = lib.next_episode("video/Show - 01").unwrap().unwrap();
        assert_eq!(next.0, "video/Show - 02");
        assert!(lib.next_episode("video/Show - 03").unwrap().is_none());
        assert_eq!(
            lib.previous_episode("video/Show - 02").unwrap().unwrap().0,
            "video/Show - 01"
        );

        let series = lib
            .video("video/Show - 01")
            .unwrap()
            .unwrap()
            .series
            .unwrap();
        lib.set_series_prefs(
            series,
            SeriesPrefs {
                audio_language: Some("jpn".into()),
                subtitle: Some(SubtitleChoice::Off),
                delay_ms: Some(300),
            },
        )
        .unwrap();
        let mut own = lib.video("video/Show - 02").unwrap().unwrap();
        own.delay_ms = Some(-100);
        lib.put_video("video/Show - 02", &own).unwrap();
        let inherited = lib.effective("video/Show - 01").unwrap();
        assert_eq!(
            (
                inherited.subtitle,
                inherited.delay_ms,
                inherited.audio_language.as_deref()
            ),
            (Some(SubtitleChoice::Off), Some(300), Some("jpn"))
        );
        assert_eq!(
            lib.effective("video/Show - 02").unwrap().delay_ms,
            Some(-100)
        );
    }
}
