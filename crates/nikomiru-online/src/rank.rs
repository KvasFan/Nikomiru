// SPDX-License-Identifier: GPL-3.0-only

//! The order of the candidates and the automatic choice (ADR 0008 §9).

use crate::language::{language_of_token, subtitle_episode, subtitle_language};
use crate::{Candidate, FileKind, ProviderId, SearchRequest, SubtitleFile};

/// Whether `language` is `wanted`, in any spelling.
fn same_language(language: &str, wanted: &str) -> bool {
    let code = |t: &str| language_of_token(t).map_or_else(|| t.to_lowercase(), str::to_string);
    code(language) == code(wanted)
}

/// The language wanted first, with the files of unknown language on Jimaku,
/// which are nearly all Japanese (ADR 0008 §8); then unknown; then others.
fn language_rank(c: &Candidate, wanted: &str) -> u8 {
    match &c.language {
        Some(l) if same_language(l, wanted) => 0,
        None if c.provider == ProviderId::Jimaku => 0,
        None => 1,
        Some(_) => 2,
    }
}

fn episode_rank(episode: Option<u32>, wanted: Option<u32>) -> u8 {
    match (wanted, episode) {
        (None, _) => 0,
        (Some(w), Some(e)) if w == e => 0,
        (Some(_), None) => 1,
        (Some(_), Some(_)) => 2,
    }
}

/// Sorts the candidates: the language wanted first (unknown on Jimaku counts
/// as it; then unknown, then the others); the episode asked for (then none,
/// then others); not machine
/// translated; subtitles before archives before the unusable; the provider
/// the user prefers (MET-011); the provider's own order.
pub fn rank(candidates: &mut [Candidate], request: &SearchRequest) {
    let preferred = |p: ProviderId| {
        request
            .providers
            .iter()
            .position(|q| *q == p)
            .unwrap_or(usize::MAX)
    };
    candidates.sort_by_key(|c| {
        (
            language_rank(c, &request.language),
            episode_rank(c.episode, request.episode),
            c.machine_translated,
            c.kind,
            preferred(c.provider),
            c.provider,
            c.order,
        )
    });
}

/// Whether the automatic choice may take `c` (MET-007): usable, not machine
/// translated, in the language wanted (or unknown on Jimaku, nearly always
/// Japanese), for the episode asked for (an archive without an episode may
/// hold it).
pub fn fits_automatic(c: &Candidate, request: &SearchRequest) -> bool {
    let language = match &c.language {
        Some(l) => same_language(l, &request.language),
        None => c.provider == ProviderId::Jimaku,
    };
    let episode = match (request.episode, c.episode) {
        (None, _) => true,
        (Some(w), Some(e)) => w == e,
        (Some(_), None) => c.kind == FileKind::Zip,
    };
    c.kind != FileKind::Unusable && !c.machine_translated && language && episode
}

/// The file to use among the subtitle files of one download (an archive may
/// hold a whole season): the episode asked for, then the language wanted;
/// `None` when the archive holds episodes but not this one.
pub fn pick_file(mut files: Vec<SubtitleFile>, request: &SearchRequest) -> Option<SubtitleFile> {
    if files.len() == 1 {
        return files.pop();
    }
    let language = |f: &SubtitleFile| match subtitle_language(&f.name) {
        Some(l) if same_language(l, &request.language) => 0u8,
        None => 1,
        Some(_) => 2,
    };
    files.sort_by(|a, b| {
        let key = |f: &SubtitleFile| {
            (
                episode_rank(subtitle_episode(&f.name), request.episode),
                language(f),
            )
        };
        key(a).cmp(&key(b)).then_with(|| a.name.cmp(&b.name))
    });
    let first = files.into_iter().next()?;
    match request.episode {
        Some(w) if subtitle_episode(&first.name).is_some_and(|e| e != w) => None,
        _ => Some(first),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Locator;

    fn c(
        provider: ProviderId,
        name: &str,
        language: Option<&str>,
        episode: Option<u32>,
    ) -> Candidate {
        Candidate {
            provider,
            locator: Locator::FileId(0),
            file_name: name.into(),
            source: String::new(),
            language: language.map(str::to_string),
            episode,
            kind: FileKind::of(name),
            machine_translated: false,
            size: None,
            order: 0,
        }
    }

    fn request(episode: Option<u32>) -> SearchRequest {
        SearchRequest {
            title: "Show".into(),
            episode,
            language: "ja".into(),
            ..SearchRequest::default()
        }
    }

    // T-MET-007
    #[test]
    fn the_language_and_the_episode_come_first() {
        use ProviderId::{Jimaku, OpenSubtitles};
        let mut all = vec![
            c(Jimaku, "Show 02.srt", None, Some(2)),
            c(OpenSubtitles, "Show 01.en.srt", Some("en"), Some(1)),
            c(Jimaku, "Show S1.zip", None, None),
            c(Jimaku, "Show 01.rar", None, Some(1)),
            c(Jimaku, "Show 01.srt", None, Some(1)),
            c(OpenSubtitles, "Show 01.srt", Some("ja"), Some(1)),
        ];
        let mut mt = c(OpenSubtitles, "Show 01 mt.srt", Some("ja"), Some(1));
        mt.machine_translated = true;
        all.push(mt);
        rank(&mut all, &request(Some(1)));
        let names: Vec<(&str, ProviderId)> = all
            .iter()
            .map(|c| (c.file_name.as_str(), c.provider))
            .collect();
        assert_eq!(
            names,
            [
                ("Show 01.srt", Jimaku),
                ("Show 01.srt", OpenSubtitles),
                ("Show 01.rar", Jimaku),
                ("Show 01 mt.srt", OpenSubtitles),
                ("Show S1.zip", Jimaku),
                ("Show 02.srt", Jimaku),
                ("Show 01.en.srt", OpenSubtitles),
            ]
        );
        let r = request(Some(1));
        let fits: Vec<&str> = all
            .iter()
            .filter(|c| fits_automatic(c, &r))
            .map(|c| c.file_name.as_str())
            .collect();
        // not machine translated, not unusable, not another episode or language
        assert_eq!(fits, ["Show 01.srt", "Show 01.srt", "Show S1.zip"]);
        // OpenSubtitles without a language is not taken
        assert!(!fits_automatic(
            &c(OpenSubtitles, "x.srt", None, Some(1)),
            &r
        ));
        // a film: any episode
        assert!(fits_automatic(
            &c(Jimaku, "Movie.srt", None, None),
            &request(None)
        ));
    }

    // T-MET-011: the provider the user prefers breaks the ties
    #[test]
    fn the_preferred_provider_comes_first_among_equals() {
        use ProviderId::{Jimaku, OpenSubtitles};
        let mut all = vec![
            c(Jimaku, "Show 01.srt", Some("ja"), Some(1)),
            c(OpenSubtitles, "Show 01.srt", Some("ja"), Some(1)),
            c(OpenSubtitles, "Show 01.en.srt", Some("en"), Some(1)),
        ];
        let mut r = request(Some(1));
        r.providers = vec![OpenSubtitles, Jimaku];
        rank(&mut all, &r);
        let order: Vec<(ProviderId, Option<&str>)> = all
            .iter()
            .map(|c| (c.provider, c.language.as_deref()))
            .collect();
        // the language still comes first
        assert_eq!(
            order,
            [
                (OpenSubtitles, Some("ja")),
                (Jimaku, Some("ja")),
                (OpenSubtitles, Some("en"))
            ]
        );
        assert_eq!(
            ProviderId::from_keys(&[
                "OpenSubtitles".into(),
                "addic7ed".into(),
                "jimaku".into(),
                "opensubtitles".into()
            ]),
            [OpenSubtitles, Jimaku]
        );
        assert_eq!(ProviderId::from_keys(&[]), []);
    }

    // T-MET-003: a season pack
    #[test]
    fn the_episode_is_taken_out_of_a_pack() {
        let files = |names: &[&str]| -> Vec<SubtitleFile> {
            names
                .iter()
                .map(|n| SubtitleFile {
                    name: (*n).into(),
                    bytes: Vec::new(),
                })
                .collect()
        };
        let pack = files(&[
            "Show - 01.ass",
            "Show - 02.en.ass",
            "Show - 02.ass",
            "Show - 03.ass",
        ]);
        assert_eq!(
            pick_file(pack.clone(), &request(Some(2))).map(|f| f.name),
            Some("Show - 02.ass".into())
        );
        assert_eq!(pick_file(pack, &request(Some(9))), None);
        assert_eq!(
            pick_file(files(&["whatever.srt"]), &request(Some(9))).map(|f| f.name),
            Some("whatever.srt".into())
        );
    }
}
