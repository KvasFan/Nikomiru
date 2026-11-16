// SPDX-License-Identifier: GPL-3.0-only

//! Bounded credential-free snapshots of mining settings (MCP-001).

use crate::config::AnkiConfig;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A named snapshot; connection settings and captured sources never belong here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MiningProfile {
    /// Unique explicit name.
    pub name: String,
    /// Existing card destination.
    pub destination: String,
    /// Deck name.
    pub deck: String,
    /// Note type name.
    pub note_type: String,
    /// Explicit tags.
    pub tags: Vec<String>,
    /// Templates for this note type only.
    pub fields: BTreeMap<String, String>,
}

/// Whether named plain-text fields/templates fit the composer limits.
pub fn valid_fields(fields: &BTreeMap<String, String>) -> bool {
    fields.len() <= 128
        && fields
            .iter()
            .all(|(k, v)| valid_name(k, 256) && v.len() <= 16 * 1024 && !v.contains('\0'))
        && fields.iter().map(|(k, v)| k.len() + v.len()).sum::<usize>() <= 256 * 1024
}
fn valid_name(s: &str, max: usize) -> bool {
    !s.is_empty() && s == s.trim() && s.len() <= max && !s.chars().any(char::is_control)
}
impl MiningProfile {
    /// Validate the complete snapshot before any active setting is changed.
    pub fn valid(&self) -> bool {
        valid_name(&self.name, 80)
            && valid_name(&self.deck, 256)
            && valid_name(&self.note_type, 256)
            && matches!(
                self.destination.as_str(),
                "anki" | "anki-later" | "nikomiru"
            )
            && self.tags.len() <= 32
            && self
                .tags
                .iter()
                .all(|t| valid_name(t, 128) && !t.chars().any(char::is_whitespace))
            && valid_fields(&self.fields)
    }
}
impl AnkiConfig {
    /// Save a new name without replacing a previous profile implicitly.
    pub fn save_profile(&mut self, name: String) -> bool {
        let p = MiningProfile {
            name,
            destination: self.destination.clone(),
            deck: self.deck.clone(),
            note_type: self.note_type.clone(),
            tags: self.tags.clone(),
            fields: self
                .field_mapping
                .get(&self.note_type)
                .cloned()
                .unwrap_or_default(),
        };
        if !p.valid() || self.profiles.len() >= 16 || self.profiles.iter().any(|x| x.name == p.name)
        {
            return false;
        }
        self.profiles.push(p);
        true
    }
    /// Explicitly apply only a validated snapshot, retaining connection/media settings.
    pub fn apply_profile(&mut self, name: &str) -> bool {
        let Some(p) = self
            .profiles
            .iter()
            .find(|p| p.name == name && p.valid())
            .cloned()
        else {
            return false;
        };
        self.destination = p.destination;
        self.deck = p.deck;
        self.note_type = p.note_type;
        self.tags = p.tags;
        self.field_mapping.insert(self.note_type.clone(), p.fields);
        self.active_profile = Some(p.name);
        true
    }
    /// Explicitly remove a profile, retaining the currently applied values.
    pub fn delete_profile(&mut self, name: &str) {
        self.profiles.retain(|p| p.name != name);
        if self.active_profile.as_deref() == Some(name) {
            self.active_profile = None;
        }
    }
    /// Validate the profile collection before accepting edited settings.
    pub fn valid_profiles(&self) -> bool {
        let mut names = std::collections::HashSet::new();
        self.profiles.len() <= 16
            && self
                .profiles
                .iter()
                .all(|p| p.valid() && names.insert(&p.name))
            && self
                .active_profile
                .as_ref()
                .is_none_or(|n| self.profiles.iter().any(|p| &p.name == n))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Config;
    #[test]
    fn profiles_roundtrip_without_connection_or_other_model_mapping() {
        let mut c = Config::default();
        c.anki.api_key = "private".into();
        c.anki.url = "http://localhost:8766".into();
        c.anki.field_mapping.insert(
            "Lapis".into(),
            BTreeMap::from([("Expression".into(), "{expression}".into())]),
        );
        c.anki.field_mapping.insert(
            "Other".into(),
            BTreeMap::from([("Secret".into(), "unrelated".into())]),
        );
        assert!(c.anki.save_profile("Reading".into()));
        assert!(!c.anki.save_profile("Reading".into()));
        let raw = toml::to_string(&c.anki.profiles[0]).unwrap();
        assert!(
            !raw.contains("private") && !raw.contains("localhost") && !raw.contains("unrelated")
        );
        c.anki.deck = "New deck".into();
        c.anki.api_key = "new private".into();
        assert!(c.anki.apply_profile("Reading"));
        assert_eq!(c.anki.deck, "Lapis");
        assert_eq!(c.anki.api_key, "new private");
        let (parsed, bad) = Config::parse(&c.to_text().unwrap()).unwrap();
        assert!(bad.is_empty());
        assert_eq!(parsed.anki, c.anki);
        c.anki.delete_profile("Reading");
        assert_eq!(c.anki.deck, "Lapis");
        assert!(c.anki.active_profile.is_none());
        assert!(
            Config::parse("[anki]\ndeck='Legacy'")
                .unwrap()
                .0
                .anki
                .profiles
                .is_empty()
        );
    }
    #[test]
    fn invalid_profiles_are_atomic_and_bounded() {
        let mut c = AnkiConfig::default();
        assert!(!c.save_profile(" ".into()));
        for i in 0..16 {
            assert!(c.save_profile(format!("P{i}")));
        }
        assert!(!c.save_profile("overflow".into()));
        c.profiles[0]
            .fields
            .insert("F".into(), "x".repeat(16 * 1024 + 1));
        let before = c.clone();
        assert!(!c.apply_profile("P0"));
        assert_eq!(c, before);
        assert!(!c.valid_profiles());
    }
}
