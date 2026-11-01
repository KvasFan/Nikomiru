// SPDX-License-Identifier: GPL-3.0-only

//! Reading the text of a manga's pages, from the interface (MNG-003…MNG-005):
//! what the app's worker does, and how far it got.

use crate::i18n::Translator;

/// What fetching the model reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OcrEvent {
    /// The bytes fetched so far.
    Progress(u64),
    /// The model is in place.
    Installed,
    /// Why it is not.
    Failed(String),
}

/// Where the reports go.
pub type Reports = Box<dyn Fn(OcrEvent) + Send + Sync>;

/// A part of a page to read.
#[derive(Debug, Clone, PartialEq)]
pub struct OcrJob {
    /// The book it is of.
    pub uid: String,
    /// The page.
    pub page: usize,
    /// Its file.
    pub picture: Vec<u8>,
    /// Left, top, width and height, as shares of the page.
    pub area: [f32; 4],
}

/// What was read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recognized {
    /// The text.
    pub text: String,
    /// The part of the page, as JPEG, for a card.
    pub crop: Vec<u8>,
}

/// What reading came to: for that book and page, the text or why not.
pub type Reading = (String, usize, Result<Recognized, String>);

/// The app's reader.
pub trait OcrService: Send {
    /// Whether the model is here.
    fn installed(&self) -> bool;
    /// Fetches the model, through `proxy` if there is one.
    fn install(&self, proxy: String, translator: Translator, reports: Reports);
    /// Reads a part of a page, off this thread.
    fn read(&self, job: OcrJob, translator: Translator, done: Box<dyn FnOnce(Reading) + Send>);
}

/// No reader: for the tests of the interface.
#[derive(Debug, Default)]
pub struct NoOcr;

impl OcrService for NoOcr {
    fn installed(&self) -> bool {
        false
    }
    fn install(&self, _: String, _: Translator, _: Reports) {}
    fn read(&self, _: OcrJob, _: Translator, _: Box<dyn FnOnce(Reading) + Send>) {}
}

/// Where reading stands on the Books page.
#[derive(Debug, Default)]
pub struct OcrState {
    /// The model is here.
    pub installed: bool,
    /// It is being fetched: the bytes so far.
    pub fetched: Option<u64>,
    /// A part of a page is being read.
    pub reading: bool,
    /// The last part read, its page and its JPEG: a card's picture.
    pub crop: Option<(usize, Vec<u8>)>,
}

/// How much of the model has come, in percent.
pub fn fetched_percent(bytes: u64) -> u64 {
    (bytes * 100 / nikomiru_ocr::model_bytes()).min(100)
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-MNG-004
    #[test]
    fn the_share_fetched() {
        assert_eq!(fetched_percent(0), 0);
        assert_eq!(fetched_percent(nikomiru_ocr::model_bytes() / 2 + 1), 50);
        assert_eq!(fetched_percent(u64::MAX / 200), 100);
    }
}
