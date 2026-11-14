// SPDX-License-Identifier: GPL-3.0-only

//! Local subtitle sources for pre-playback comprehension scores (KNW-008).
//! Probe only when the first language's external file cannot decide the
//! choice. Embedded text is extracted on the counter's own worker and cache;
//! graphic tracks never start OCR. Both processes share its shutdown token.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use nikomiru_core::Cancel;
use nikomiru_library::SubtitleChoice;
use nikomiru_media::extract::{Extractor, Stream, is_text_codec};
use nikomiru_media::process::{Job, file_arg, output};
use nikomiru_media::tools::{Search, find};
use nikomiru_subs::select::{self, Choice, EmbeddedTrack, External};
use nikomiru_ui::i18n::Translator;
use nikomiru_ui::scores::{self, CountRequest};
use serde::Deserialize;

const KEPT: usize = 256;
type FileKey = (PathBuf, u64, Option<SystemTime>);

#[derive(Debug, Clone, Default, Deserialize)]
struct Tags {
    language: Option<String>,
    title: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct Track {
    index: i64,
    codec_name: String,
    #[serde(default)]
    tags: Tags,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct Format {
    duration: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct Metadata {
    #[serde(default)]
    streams: Vec<Track>,
    #[serde(default)]
    format: Format,
}

impl Metadata {
    fn choose(
        &self,
        external: &[External],
        saved: Option<&SubtitleChoice>,
        languages: &[String],
    ) -> Option<Choice> {
        if saved == Some(&SubtitleChoice::Off) {
            return None;
        }
        let saved = match saved {
            Some(SubtitleChoice::File(path)) => Some(Choice::External(path.into())),
            Some(SubtitleChoice::Embedded {
                ff_index,
                language,
                title,
            }) => {
                // Same identity rules as Session::saved_choice: an index
                // needs its language too; otherwise match language/title.
                self.streams
                    .iter()
                    .find(|t| Some(t.index) == *ff_index && t.tags.language == *language)
                    .or_else(|| {
                        self.streams
                            .iter()
                            .find(|t| t.tags.language == *language && t.tags.title == *title)
                    })
                    .map(|t| Choice::Embedded(t.index))
            }
            _ => None,
        };
        let tracks: Vec<_> = self
            .streams
            .iter()
            .map(|t| EmbeddedTrack {
                // ff-index order preserves the player's subtitle-track order.
                id: t.index,
                language: t.tags.language.clone(),
                title: t.tags.title.clone(),
                text: is_text_codec(&t.codec_name),
            })
            .collect();
        languages
            .iter()
            .find_map(|lang| select::choose(saved.as_ref(), external, &tracks, lang))
    }

    fn duration(&self) -> Duration {
        self.format
            .duration
            .as_deref()
            .and_then(|s| s.parse().ok())
            .and_then(|s| Duration::try_from_secs_f64(s).ok())
            .unwrap_or_default()
    }
}

/// The counter's bounded probe cache and private extraction directory.
#[derive(Debug)]
pub struct Resolver {
    cache_dir: PathBuf,
    probed: HashMap<FileKey, Metadata>,
}

impl Resolver {
    /// A cache distinct from the playback loader's to avoid competing writes.
    pub fn new(cache_dir: PathBuf) -> Self {
        Self {
            cache_dir,
            probed: HashMap::new(),
        }
    }

    /// The local text subtitle the player would choose, extracted if needed.
    /// Off, remote, missing and graphic-only sources have no text score.
    pub fn resolve(
        &mut self,
        request: &CountRequest,
        cancel: &Cancel,
        tr: &Translator,
    ) -> Result<Option<PathBuf>, String> {
        if cancel.is_cancelled() {
            return Err("closing".into());
        }
        if matches!(&request.saved, Some(nikomiru_library::SubtitleChoice::File(path)) if std::path::Path::new(path).extension().is_some_and(|s| matches!(s.to_string_lossy().to_ascii_lowercase().as_str(), "idx" | "sub")))
        {
            return Ok(None);
        }
        let video = &request.video;
        if !video.is_file()
            || request.saved == Some(SubtitleChoice::Off)
            || request.preferred.is_empty()
        {
            return Ok(None);
        }
        let downloaded = nikomiru_online::hash::video_hash(video)
            .map(|h| {
                nikomiru_online::store::list(&nikomiru_online::store::folder(
                    &request.subtitles_dir,
                    &h,
                ))
            })
            .unwrap_or_default();
        let external = scores::subtitle_files(video, &downloaded);
        if let Some(file) =
            scores::decisive_file(&external, request.saved.as_ref(), &request.preferred)
        {
            return Ok(Some(file));
        }
        let meta = std::fs::metadata(video).map_err(|e| e.to_string())?;
        let key = (video.clone(), meta.len(), meta.modified().ok());
        let (_, ffmpeg, ffprobe) =
            find(&Search::from_env(&request.ffmpeg)).map_err(|e| tr.error(&e))?;
        if !self.probed.contains_key(&key) {
            let mut job = Job::new(
                ffprobe,
                [
                    "-v",
                    "error",
                    "-select_streams",
                    "s",
                    "-show_entries",
                    "stream=index,codec_name:stream_tags=language,title:format=duration",
                    "-of",
                    "json",
                ],
            );
            job.args.push(file_arg(video));
            let metadata: Metadata =
                serde_json::from_str(&output(&job, cancel).map_err(|e| tr.error(&e))?)
                    .map_err(|e| e.to_string())?;
            if self.probed.len() >= KEPT {
                self.probed.clear();
            }
            self.probed.insert(key.clone(), metadata);
        }
        let metadata = &self.probed[&key];
        match metadata.choose(&external, request.saved.as_ref(), &request.preferred) {
            Some(Choice::External(file)) => Ok(Some(file)),
            Some(Choice::Embedded(index)) => {
                let Some(track) = metadata
                    .streams
                    .iter()
                    .find(|t| t.index == index && is_text_codec(&t.codec_name))
                else {
                    return Ok(None);
                };
                let extractor = Extractor {
                    ffmpeg,
                    cache_dir: self.cache_dir.clone(),
                };
                let files = extractor
                    .extract(
                        video,
                        &[Stream {
                            ff_index: index,
                            codec: track.codec_name.clone(),
                        }],
                        metadata.duration(),
                        cancel,
                    )
                    .map_err(|e| tr.error(&e))?;
                Ok(files
                    .into_iter()
                    .find(|(i, _)| *i == index)
                    .map(|(_, file)| file))
            }
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metadata() -> Metadata {
        serde_json::from_str(
            r#"{"streams":[
            {"index":3,"codec_name":"subrip","tags":{"language":"jpn","title":"SRT"}},
            {"index":4,"codec_name":"ass","tags":{"language":"jpn","title":"ASS"}},
            {"index":5,"codec_name":"subrip","tags":{"language":"eng","title":"English"}},
            {"index":6,"codec_name":"hdmv_pgs_subtitle","tags":{"language":"jpn","title":"PGS"}}
        ],"format":{"duration":"12.5"}}"#,
        )
        .unwrap()
    }

    fn file(path: &str, language: Option<&str>) -> External {
        External {
            path: path.into(),
            format: nikomiru_subs::Format::Srt,
            language: language.map(String::from),
        }
    }

    // T-KNW-008: first-language embedded text precedes later-language files.
    #[test]
    fn selection_matches_the_player_across_languages_and_sources() {
        let metadata = metadata();
        let langs = ["ja".into(), "en".into()];
        let files = [file("ep.en.srt", Some("en"))];
        assert_eq!(
            metadata.choose(&files, None, &langs),
            Some(Choice::Embedded(3))
        );
        assert_eq!(
            metadata.choose(&files, None, &["en".into()]),
            Some(Choice::External("ep.en.srt".into()))
        );
        assert_eq!(
            metadata.choose(&[file("ep.ja.srt", Some("ja"))], None, &langs),
            Some(Choice::External("ep.ja.srt".into()))
        );
        assert_eq!(
            metadata.choose(&[file("ep.srt", None)], None, &langs),
            Some(Choice::External("ep.srt".into()))
        );
        assert_eq!(metadata.choose(&[], None, &["ko".into()]), None);
        assert_eq!(metadata.choose(&files, None, &[]), None);
    }

    // T-KNW-008: saved index+language, then language+title, then auto choice.
    #[test]
    fn saved_track_identity_and_off_match_the_player() {
        let metadata = metadata();
        let langs = ["ja".into()];
        let files = [file("ep.en.srt", Some("en"))];
        let saved = |index, language: &str, title: &str| SubtitleChoice::Embedded {
            ff_index: index,
            language: Some(language.into()),
            title: Some(title.into()),
        };
        assert_eq!(
            metadata.choose(&files, Some(&saved(Some(4), "jpn", "old title")), &langs),
            Some(Choice::Embedded(4))
        );
        assert_eq!(
            metadata.choose(&files, Some(&saved(Some(99), "jpn", "ASS")), &langs),
            Some(Choice::Embedded(4))
        );
        assert_eq!(
            metadata.choose(&files, Some(&saved(Some(3), "eng", "English")), &langs),
            Some(Choice::Embedded(5)),
            "index alone must not select a different language"
        );
        assert_eq!(
            metadata.choose(&files, Some(&saved(Some(99), "kor", "gone")), &langs),
            Some(Choice::Embedded(3))
        );
        assert_eq!(
            metadata.choose(
                &files,
                Some(&SubtitleChoice::File("ep.en.srt".into())),
                &langs
            ),
            Some(Choice::External("ep.en.srt".into()))
        );
        assert_eq!(
            metadata.choose(
                &files,
                Some(&SubtitleChoice::File("gone.srt".into())),
                &langs
            ),
            Some(Choice::Embedded(3))
        );
        assert_eq!(
            metadata.choose(&files, Some(&SubtitleChoice::Off), &langs),
            None
        );
    }

    // T-KNW-008: selecting images must not silently score another track.
    #[test]
    fn an_explicit_graphic_track_stays_graphic() {
        let metadata = metadata();
        let saved = SubtitleChoice::Embedded {
            ff_index: Some(6),
            language: Some("jpn".into()),
            title: Some("PGS".into()),
        };
        assert_eq!(
            metadata.choose(&[], Some(&saved), &["ja".into()]),
            Some(Choice::Embedded(6))
        );
        assert!(!is_text_codec(&metadata.streams[3].codec_name));
    }

    // T-KNW-008: unknown or invalid durations use the bounded default timeout.
    #[test]
    fn duration_metadata_cannot_panic() {
        let mut metadata = metadata();
        assert_eq!(metadata.duration(), Duration::from_millis(12_500));
        for duration in [
            None,
            Some("N/A"),
            Some("-1"),
            Some("NaN"),
            Some("inf"),
            Some("1e100"),
        ] {
            metadata.format.duration = duration.map(String::from);
            assert_eq!(metadata.duration(), Duration::ZERO);
        }
    }
}
