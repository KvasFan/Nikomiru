// SPDX-License-Identifier: GPL-3.0-only

//! The text of games from the interface (SPECIFICATION §6.15): what the app's
//! hooker worker reports, and the lines of the Texthooker page.

use nikomiru_core::config::HookerConfig;

/// How many lines the page keeps.
pub const LINES: usize = 500;

/// What the worker reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookerEvent {
    /// A line.
    Line(String),
    /// The hooker answered, or went.
    Connected(bool),
    /// No program reads the clipboard here (HOK-003).
    NoClipboard,
}

/// Where the reports go.
pub type Reports = Box<dyn Fn(HookerEvent) + Send + Sync>;

/// The app's hooker worker: it runs only while the page is open (HOK-001).
pub trait HookerService: Send {
    /// Starts reading, in place of any earlier reading.
    fn start(&self, config: HookerConfig, reports: Reports);
    /// Stops.
    fn stop(&self);
}

/// No hooker: for the tests of the interface.
#[derive(Debug, Default)]
pub struct NoHooker;

impl HookerService for NoHooker {
    fn start(&self, _: HookerConfig, _: Reports) {}
    fn stop(&self) {}
}

/// The Texthooker page.
#[derive(Debug, Default)]
pub struct HookerPage {
    /// The lines, the last one newest.
    pub lines: Vec<String>,
    /// A hooker is connected.
    pub connected: bool,
    /// The page's message.
    pub status: Option<(String, bool)>,
}

impl HookerPage {
    /// Adds a line, unless it repeats the last one: hookers send a line again
    /// when a game redraws it.
    pub fn push(&mut self, line: String) {
        if self.lines.last() == Some(&line) {
            return;
        }
        self.lines.push(line);
        if self.lines.len() > LINES {
            let extra = self.lines.len() - LINES;
            self.lines.drain(..extra);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-HOK-002
    #[test]
    fn a_repeated_line_counts_once_and_old_ones_go() {
        let mut page = HookerPage::default();
        page.push("a".into());
        page.push("a".into());
        page.push("b".into());
        page.push("a".into());
        assert_eq!(page.lines, ["a", "b", "a"]);
        for n in 0..LINES + 5 {
            page.push(n.to_string());
        }
        assert_eq!(page.lines.len(), LINES);
        assert_eq!(page.lines[0], "5");
    }
}
