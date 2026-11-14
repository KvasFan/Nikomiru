// SPDX-License-Identifier: GPL-3.0-only

//! Whole-book counts on the dedicated dictionary worker (KNW-016).

use nikomiru_core::Cancel;
use nikomiru_dict::segment::segment;
use nikomiru_library::{DictionaryError, DictionaryManager};
use nikomiru_ui::book_scores::{Outcome, Request};
use nikomiru_ui::i18n::Translator;

/// Count every text paragraph of a bounded local book. Image bytes are not
/// read; cancellation and limits never return a partial count.
pub fn count(
    manager: &DictionaryManager,
    request: &Request,
    cancel: &Cancel,
    tr: &Translator,
) -> Result<Outcome, String> {
    if cancel.is_cancelled() {
        return Err("closing".into());
    }
    let read = nikomiru_books::analysis::read_text(&request.path, cancel);
    if cancel.is_cancelled() {
        return Err("closing".into());
    }
    let book = match read {
        Ok(book) => book,
        Err(nikomiru_books::BookError::TooLarge) => return Ok(Outcome::TooLarge),
        Err(nikomiru_books::BookError::NoText) => return Ok(Outcome::NoText),
        Err(error) => return Err(tr.error(&error)),
    };
    let uid = nikomiru_library::books::book_uid(&book.title, &book.author, book.chars());
    if uid != request.uid {
        return Err(tr.tr("books-score-source-changed", &[]));
    }
    let lookup = manager.lookup().map_err(|e| tr.error(&e))?;
    let texts = book
        .chapters
        .into_iter()
        .flat_map(|c| c.paragraphs)
        .filter(|p| p.image.is_none())
        .map(|p| p.text);
    let counted = nikomiru_ui::scores::count(texts, |text| {
        if cancel.is_cancelled() {
            return Err("closing".into());
        }
        segment(&lookup, text).map_err(|e| tr.error(&DictionaryError::from(e)))
    })?;
    if cancel.is_cancelled() {
        return Err("closing".into());
    }
    Ok(Outcome::Counted(counted))
}

/// Reuse complete persisted counts after validating source and dictionary identity.
pub fn count_cached(
    manager: &DictionaryManager,
    request: &Request,
    cancel: &Cancel,
    tr: &Translator,
    cache: &crate::analysis_cache::Store,
) -> Result<Outcome, String> {
    if cancel.is_cancelled() {
        return Err("cancelled".into());
    }
    let before = crate::analysis_cache::key(manager, &request.path, &request.uid, cancel);
    if let Some(key) = &before
        && let Some(counted) = cache.get(key, cancel)
    {
        tracing::debug!(book = %request.uid, "book count cache hit");
        return Ok(Outcome::Counted(counted));
    }
    let result = count(manager, request, cancel, tr)?;
    if let (Some(key), Outcome::Counted(counted)) = (&before, &result) {
        if crate::analysis_cache::key(manager, &request.path, &request.uid, cancel).as_ref()
            != Some(key)
        {
            return Err(tr.tr("analysis-source-changed", &[]));
        }
        cache.put(key, counted, cancel);
    }
    Ok(result)
}
