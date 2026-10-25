// SPDX-License-Identifier: GPL-3.0-only

//! A search across the providers, a download, and the automatic subtitles
//! (MET-007, MET-009, ADR 0008 §9, §11).

use crate::archive::subtitle_files;
use crate::error::OnlineError;
use crate::rank::{fits_automatic, pick_file, rank};
use crate::{Candidate, FileKind, Provider, ProviderId, SearchRequest, SubtitleFile};

/// The candidates automatic subtitles try at most: a download that does not
/// hold the episode is followed by the next.
const AUTOMATIC_TRIES: usize = 3;

/// What a search found.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Found {
    /// The candidates of every provider, ranked.
    pub candidates: Vec<Candidate>,
    /// The providers that failed, and why.
    pub failed: Vec<(ProviderId, OnlineError)>,
}

/// Searches every provider in turn.
pub fn search_all(providers: &mut [Box<dyn Provider>], request: &SearchRequest) -> Found {
    let mut found = Found::default();
    for p in providers.iter_mut() {
        match p.search(request) {
            Ok(c) => found.candidates.extend(c),
            Err(e) => {
                tracing::info!("{}: {e}", p.id().name());
                found.failed.push((p.id(), e));
            }
        }
    }
    rank(&mut found.candidates, request);
    found
}

/// Downloads `candidate` with its provider and returns the subtitle file to
/// use: the file itself, or the best one of a ZIP archive.
pub fn fetch(
    providers: &mut [Box<dyn Provider>],
    candidate: &Candidate,
    request: &SearchRequest,
) -> Result<SubtitleFile, OnlineError> {
    if candidate.kind == FileKind::Unusable {
        return Err(OnlineError::Unusable(candidate.file_name.clone()));
    }
    let provider = providers
        .iter_mut()
        .find(|p| p.id() == candidate.provider)
        .ok_or_else(|| OnlineError::NoKey {
            provider: candidate.provider.name().into(),
        })?;
    let bytes = provider.download(candidate)?;
    match candidate.kind {
        FileKind::Zip => pick_file(subtitle_files(&bytes)?, request).ok_or_else(|| {
            OnlineError::Archive(format!(
                "no subtitles for episode {} inside",
                request.episode.unwrap_or_default()
            ))
        }),
        _ => Ok(SubtitleFile {
            name: candidate.file_name.clone(),
            bytes,
        }),
    }
}

/// Automatic subtitles (MET-007): the best candidates that fit, downloaded in
/// turn until one gives a file. `Ok(None)` when nothing fits; an error when
/// every provider failed.
pub fn automatic(
    providers: &mut [Box<dyn Provider>],
    request: &SearchRequest,
) -> Result<Option<(Candidate, SubtitleFile)>, OnlineError> {
    let mut found = search_all(providers, request);
    if !providers.is_empty() && found.failed.len() == providers.len() {
        return Err(found.failed.swap_remove(0).1);
    }
    for candidate in found
        .candidates
        .iter()
        .filter(|c| fits_automatic(c, request))
        .take(AUTOMATIC_TRIES)
    {
        match fetch(providers, candidate, request) {
            Ok(file) => return Ok(Some((candidate.clone(), file))),
            Err(e) => tracing::info!(
                "{}: {}: {e}",
                candidate.provider.name(),
                candidate.file_name
            ),
        }
    }
    Ok(None)
}
