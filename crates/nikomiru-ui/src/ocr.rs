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
    /// Imported text skips OCR; the worker only prepares its image crop.
    pub preset_text: Option<String>,
    /// Original imported integer crop coordinates, when supplied.
    pub pixel_bounds: Option<[u32; 4]>,
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

/// A whole page to find the text on and read, ahead of the reader (MNG-011).
#[derive(Debug, Clone, PartialEq)]
pub struct PageJob {
    /// The book it is of.
    pub uid: String,
    /// The page.
    pub page: usize,
    /// Its file.
    pub picture: Vec<u8>,
}

/// A page read ahead: for that book and page, its blocks or why not.
pub type PageReading = (String, usize, Result<nikomiru_books::mokuro::Page, String>);

/// Where pages read ahead go.
pub type PageReports = std::sync::Arc<dyn Fn(PageReading) + Send + Sync>;

/// The app's reader.
pub trait OcrService: Send {
    /// Whether the model is here.
    fn installed(&self) -> bool;
    /// Fetches the model, through `proxy` if there is one.
    fn install(&self, proxy: String, translator: Translator, reports: Reports);
    /// Reads a part of a page, off this thread.
    fn read(&self, job: OcrJob, translator: Translator, done: Box<dyn FnOnce(Reading) + Send>);
    /// Loads the model off this thread, when it is installed and not loaded
    /// yet, so that the first reading does not wait for it (MNG-003).
    fn warm(&self) {}
    /// Whether whole pages can be read ahead: the detector is here too
    /// (MNG-011).
    fn pages_installed(&self) -> bool {
        false
    }
    /// Reads `pages` ahead, in that order, off this thread, in place of the
    /// pages asked for before and not begun; a part of a page asked for
    /// with [`OcrService::read`] goes first.
    fn prepare(&self, pages: Vec<PageJob>, done: PageReports) {
        let _ = (pages, done);
    }
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
    /// The detector is here too: pages are read ahead (MNG-011).
    pub pages: bool,
    /// The pages of the open manga read ahead, and what was found on them.
    pub found: std::collections::BTreeMap<usize, nikomiru_books::mokuro::Page>,
    /// The pages being read ahead.
    pub preparing: std::collections::BTreeSet<usize>,
    /// The worker was given pages that may not all be read yet.
    pub queued: bool,
    /// Pages forgotten for want of memory, and the page shown then: they are
    /// not read ahead again until the reader turns a page (MNG-013).
    pub forgotten: (usize, std::collections::BTreeSet<usize>),
}

impl OcrState {
    /// What reading ahead knew of the manga closed is forgotten.
    pub fn forget_pages(&mut self) {
        self.found.clear();
        self.preparing.clear();
        self.forgotten.1.clear();
    }
}

/// About the memory a page read takes: its text, its lines' boxes, and what
/// holds them (MNG-013).
pub fn page_bytes(page: &nikomiru_books::mokuro::Page) -> usize {
    let regions: usize = page
        .regions
        .iter()
        .map(|r| {
            std::mem::size_of::<nikomiru_books::mokuro::Region>()
                + r.text.len()
                + r.lines.len() * std::mem::size_of::<(std::ops::Range<usize>, [u32; 4])>()
        })
        .sum();
    std::mem::size_of::<nikomiru_books::mokuro::Page>() + regions + 32
}

/// The memory all the pages read take.
pub fn pages_bytes(
    found: &std::collections::BTreeMap<usize, nikomiru_books::mokuro::Page>,
) -> usize {
    found.values().map(page_bytes).sum()
}

/// Pages read are forgotten, the farthest from `shown` first, until they
/// fit in `limit` bytes; those shown are kept whatever they take (MNG-013).
/// A page forgotten is in the disk cache still, and read again from there
/// when the reader comes back to it.
pub fn forget_far(
    found: &mut std::collections::BTreeMap<usize, nikomiru_books::mokuro::Page>,
    shown: std::ops::Range<usize>,
    limit: usize,
) -> Vec<usize> {
    let mut gone = Vec::new();
    let mut total = pages_bytes(found);
    while total > limit {
        let far = found
            .keys()
            .copied()
            .filter(|page| !shown.contains(page))
            // as far behind as ahead: the one behind, as reading goes on
            .max_by_key(|page| (page.abs_diff(shown.start), *page < shown.start));
        let Some(page) = far else {
            break;
        };
        if let Some(read) = found.remove(&page) {
            total -= page_bytes(&read);
            gone.push(page);
        }
    }
    gone
}

/// How much of the model has come, in percent.
pub fn fetched_percent(bytes: u64) -> u64 {
    (bytes * 100 / nikomiru_ocr::model_bytes()).min(100)
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-MNG-013: the pages farthest from those shown go first, the shown
    // ones never
    #[test]
    fn pages_read_fit_in_their_memory() {
        let page = |text: &str| nikomiru_books::mokuro::Page {
            width: 10,
            height: 10,
            regions: vec![nikomiru_books::mokuro::Region::guessed(
                [0, 0, 10, 10],
                text.into(),
                true,
            )],
        };
        let mut found: std::collections::BTreeMap<_, _> =
            (0..10).map(|p| (p, page("猫です"))).collect();
        let one = page_bytes(&found[&0]);
        assert!(one > 9 && pages_bytes(&found) == 10 * one);
        assert_eq!(forget_far(&mut found, 3..5, 6 * one), vec![9, 8, 7, 0]);
        assert_eq!(
            found.keys().copied().collect::<Vec<_>>(),
            vec![1, 2, 3, 4, 5, 6]
        );
        forget_far(&mut found, 3..5, 0);
        assert_eq!(found.keys().copied().collect::<Vec<_>>(), vec![3, 4]);
    }

    // T-MNG-004
    #[test]
    fn the_share_fetched() {
        assert_eq!(fetched_percent(0), 0);
        assert_eq!(fetched_percent(nikomiru_ocr::model_bytes() / 2 + 1), 50);
        assert_eq!(fetched_percent(u64::MAX / 200), 100);
    }
}
