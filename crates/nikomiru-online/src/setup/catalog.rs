// SPDX-License-Identifier: GPL-3.0-only

//! The catalogue of the setup assistant (INS-002, INS-003), shipped with
//! Nikomiru in `assets/setup/catalog.json`: the free dictionaries, where
//! their makers publish them, and the audio libraries of Fushi's pack.

use serde::Deserialize;

const CATALOG: &str = include_str!("../../../../assets/setup/catalog.json");

/// A dictionary of the catalogue.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Dictionary {
    /// Its id in the catalogue.
    pub id: String,
    /// Its name.
    pub name: String,
    /// `term`, `kanji`, `pitch`, `frequency` or `names`.
    pub kind: String,
    /// The start of its title once imported: a dictionary installed with
    /// such a title is this one.
    pub title_prefix: String,
    /// Its size, about.
    pub size_bytes: u64,
    /// Its license; empty when its source states none.
    pub license: String,
    /// Where its makers are.
    pub source: String,
    /// Where it is downloaded from.
    pub url: String,
    /// Its SHA-256, for a file that does not change.
    #[serde(default)]
    pub sha256: Option<String>,
    /// Offered ticked.
    pub ticked: bool,
}

/// An audio library of Fushi's pack.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AudioLibrary {
    /// Its id in the catalogue.
    pub id: String,
    /// Its name in the pack's `fushi.db`, and the file it becomes.
    pub name: String,
    /// `japanese` or `english`.
    pub kind: String,
    /// Its size, about: the pack's says.
    pub size_bytes: u64,
    /// Offered ticked.
    pub ticked: bool,
}

/// The catalogue.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Catalog {
    /// Its format.
    pub version: u32,
    /// The dictionaries, in the order offered.
    pub dictionaries: Vec<Dictionary>,
    /// The audio libraries.
    pub audio: Vec<AudioLibrary>,
}

/// The catalogue shipped with Nikomiru.
pub fn catalog() -> Catalog {
    serde_json::from_str(CATALOG).unwrap_or(Catalog {
        version: 0,
        dictionaries: Vec::new(),
        audio: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-INS-002: the catalogue reads; HTTPS only; the defaults of INS-002
    #[test]
    fn the_catalogue() {
        let c = catalog();
        assert_eq!(c.version, 1);
        let ticked: Vec<&str> = c
            .dictionaries
            .iter()
            .filter(|d| d.ticked)
            .map(|d| d.id.as_str())
            .collect();
        assert_eq!(ticked, ["jitendex", "kanjidic", "kanjium", "jpdb"]);
        for d in &c.dictionaries {
            assert!(d.url.starts_with("https://"), "{}", d.id);
            assert!(d.sha256.as_ref().is_none_or(|s| s.len() == 64), "{}", d.id);
        }
        let audio: Vec<(&str, bool)> = c
            .audio
            .iter()
            .map(|a| (a.name.as_str(), a.ticked))
            .collect();
        assert_eq!(audio, [("android.db", true), ("android_english.db", false)]);
    }
}
