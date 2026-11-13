// SPDX-License-Identifier: GPL-3.0-only

//! Library episode choices and condensed-audio exports (AUD-010).

use nikomiru_library::{Library, SubtitleChoice};
use nikomiru_media::process::{Job, file_arg, output};
use nikomiru_media::{
    condense::{self, Line, Plan},
    extract::{Extractor, Stream},
};
use nikomiru_subs::{
    select::{self, Choice, EmbeddedTrack},
    timeline::Timeline,
};
use nikomiru_ui::{
    condense::{Report, Request, Snapshot, Target},
    i18n::Translator,
};
use std::path::{Path, PathBuf};

/// Pad a parsed track with the same rule as card clips, retaining chapter text.
pub fn lines(t: &Timeline, head: i64, tail: i64) -> Vec<Line> {
    t.cues()
        .iter()
        .enumerate()
        .map(|(i, q)| {
            let (start_ms, end_ms) = t.clip(i, head, tail);
            Line {
                start_ms,
                end_ms,
                text: q.plain_text.clone(),
            }
        })
        .collect()
}

fn episode(
    lib: &Library,
    uid: &str,
    r: &Request,
    ffmpeg: &Path,
    ffprobe: &Path,
    cache: &Path,
    tr: &Translator,
) -> Result<Snapshot, String> {
    let missing = || tr.tr("condense-no-subtitles", &[]);
    let video = lib
        .video(uid)
        .map_err(|e| tr.error(&e))?
        .ok_or_else(missing)?;
    let path = PathBuf::from(&video.path);
    if !path.is_file() {
        return Err(tr.tr("condense-local-only", &[]));
    }
    let prefs = lib.effective(uid).map_err(|e| tr.error(&e))?;
    if prefs.subtitle == Some(SubtitleChoice::Off) {
        return Err(missing());
    }
    let mut probe = Job::new(
        ffprobe,
        [
            "-v",
            "error",
            "-show_streams",
            "-show_format",
            "-of",
            "json",
        ],
    );
    probe.args.push(file_arg(&path));
    let metadata: serde_json::Value =
        serde_json::from_str(&output(&probe, &r.cancel).map_err(|e| tr.error(&e))?)
            .map_err(|e| e.to_string())?;
    let streams = metadata["streams"].as_array().ok_or_else(missing)?;
    let duration_ms = metadata["format"]["duration"]
        .as_str()
        .and_then(|s| s.parse::<f64>().ok())
        .filter(|s| s.is_finite() && *s > 0.0)
        .map(|s| (s * 1000.0).round() as i64)
        .or(video.duration_ms.map(|s| s as i64))
        .ok_or_else(missing)?;
    let audio: Vec<_> = streams
        .iter()
        .filter(|s| s["codec_type"] == "audio")
        .collect();
    let sound = video
        .aid
        .and_then(|id| usize::try_from(id - 1).ok())
        .and_then(|i| audio.get(i).copied())
        .or_else(|| {
            prefs.audio_language.as_ref().and_then(|lang| {
                audio.iter().copied().find(|s| {
                    s["tags"]["language"]
                        .as_str()
                        .is_some_and(|tag| select::same_language(lang, tag))
                })
            })
        })
        .or_else(|| audio.first().copied())
        .ok_or_else(|| tr.tr("condense-no-audio", &[]))?;
    let embedded: Vec<_> = streams
        .iter()
        .filter(|s| s["codec_type"] == "subtitle")
        .collect();
    let tracks: Vec<_> = embedded
        .iter()
        .enumerate()
        .map(|(i, s)| EmbeddedTrack {
            id: i as i64 + 1,
            language: s["tags"]["language"].as_str().map(str::to_string),
            title: s["tags"]["title"].as_str().map(str::to_string),
            text: nikomiru_media::extract::is_text_codec(s["codec_name"].as_str().unwrap_or("")),
        })
        .collect();
    let downloaded = nikomiru_online::hash::video_hash(&path)
        .ok()
        .map(|h| {
            nikomiru_online::store::list(&nikomiru_online::store::folder(&r.subtitles_dir, &h))
        })
        .unwrap_or_default();
    let mut external = nikomiru_ui::scores::subtitle_files(&path, &downloaded);
    let saved = match &prefs.subtitle {
        Some(SubtitleChoice::File(p)) => {
            let p = PathBuf::from(p);
            if p.is_file() && !external.iter().any(|e| e.path == p) {
                external.push(select::External {
                    path: p.clone(),
                    format: nikomiru_subs::Format::Srt,
                    language: None,
                });
            }
            Some(Choice::External(p))
        }
        Some(SubtitleChoice::Embedded {
            ff_index,
            language,
            title,
        }) => embedded
            .iter()
            .enumerate()
            .find(|(_, s)| ff_index.is_some_and(|i| s["index"].as_i64() == Some(i)))
            .or_else(|| {
                embedded.iter().enumerate().find(|(_, s)| {
                    language.as_deref() == s["tags"]["language"].as_str()
                        && title.as_deref() == s["tags"]["title"].as_str()
                })
            })
            .map(|(i, _)| Choice::Embedded(i as i64 + 1)),
        _ => None,
    };
    let chosen = r
        .config
        .subtitles
        .preferred_languages
        .iter()
        .find_map(|lang| select::choose(saved.as_ref(), &external, &tracks, lang))
        .ok_or_else(missing)?;
    let subtitle = match chosen {
        Choice::External(p) => p,
        Choice::Embedded(id) => {
            let s = embedded.get((id - 1) as usize).ok_or_else(missing)?;
            let codec = s["codec_name"].as_str().unwrap_or("");
            if !nikomiru_media::extract::is_text_codec(codec) {
                return Err(missing());
            }
            let index = s["index"].as_i64().ok_or_else(missing)?;
            let extractor = Extractor {
                ffmpeg: ffmpeg.to_path_buf(),
                cache_dir: cache.join("condense-subs"),
            };
            extractor
                .extract(
                    &path,
                    &[Stream {
                        ff_index: index,
                        codec: codec.into(),
                    }],
                    std::time::Duration::from_millis(duration_ms as u64),
                    &r.cancel,
                )
                .map_err(|e| tr.error(&e))?
                .into_iter()
                .find(|(i, _)| *i == index)
                .map(|(_, p)| p)
                .ok_or_else(missing)?
        }
    };
    let loaded = crate::subtitles::read_file(&subtitle, None, tr)?;
    let mut t = Timeline::new(loaded.track.cues);
    t.delay_ms = prefs.delay_ms.unwrap_or(0);
    Ok(Snapshot {
        input: path,
        stream: sound["index"].as_i64(),
        duration_ms,
        lines: lines(
            &t,
            i64::from(r.config.anki.audio_pad_head_ms),
            i64::from(r.config.anki.audio_pad_tail_ms),
        ),
    })
}

/// Run an export on the library thread, retaining per-episode failures.
pub fn run(
    lib: &Library,
    r: &Request,
    ffmpeg: &Path,
    ffprobe: &Path,
    cache: &Path,
    tr: &Translator,
) -> Result<Report, String> {
    let mut report = Report::default();
    let episodes = match &r.target {
        Target::Current => vec![(
            String::new(),
            r.current
                .as_ref()
                .map(|s| s.input.display().to_string())
                .unwrap_or_default(),
        )],
        Target::Episode(uid) => vec![(
            uid.clone(),
            lib.video(uid)
                .map_err(|e| tr.error(&e))?
                .map(|v| v.path)
                .unwrap_or_else(|| uid.clone()),
        )],
        Target::Series(id) => lib
            .episodes(*id)
            .map_err(|e| tr.error(&e))?
            .into_iter()
            .map(|(uid, v)| (uid, v.path))
            .collect(),
    };
    for (uid, name) in episodes {
        if r.cancel.is_cancelled() {
            break;
        }
        let snapshot = if r.target == Target::Current {
            r.current
                .clone()
                .ok_or_else(|| tr.tr("condense-no-subtitles", &[]))
        } else {
            episode(lib, &uid, r, ffmpeg, ffprobe, cache, tr)
        };
        let result = snapshot.and_then(|s| {
            let plan = Plan::new(&s.lines, s.duration_ms);
            if plan.spans.is_empty() {
                return Err(tr.tr("condense-no-subtitles", &[]));
            }
            condense::export(
                ffmpeg, &s.input, s.stream, &plan, &r.folder, r.format, &r.cancel,
            )
            .map_err(|reason| tr.tr("condense-failed", &[("reason", reason)]))
        });
        match result {
            Ok(path) => report.written.push(path),
            Err(e) => report.failed.push((name, e)),
        }
    }
    Ok(report)
}
