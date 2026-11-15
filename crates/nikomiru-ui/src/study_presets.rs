// SPDX-License-Identifier: GPL-3.0-only

//! Bounded portable filter presets, excluding paths, services and secrets (KNW-052).

use nikomiru_core::{
    Config,
    config::{StudyPreset, SubtitleStudyPreset},
};
use serde::{Deserialize, Serialize};

/// Maximum complete portable preset file size, including JSON escaping.
pub const MAX_BYTES: usize = 1 << 20;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Bundle {
    format: u32,
    #[serde(default)]
    vocabulary: Vec<StudyPreset>,
    #[serde(default)]
    subtitles: Vec<SubtitleStudyPreset>,
}
fn validate(bundle: &Bundle) -> Result<(), &'static str> {
    if bundle.format != 1 {
        return Err("study-presets-unsupported");
    }
    if bundle.vocabulary.len() > 16 || bundle.subtitles.len() > 16 {
        return Err("study-presets-too-large");
    }
    let mut names = std::collections::HashSet::new();
    if bundle
        .vocabulary
        .iter()
        .any(|p| !p.valid() || !names.insert(p.name.as_str()))
    {
        return Err("study-presets-invalid");
    }
    names.clear();
    if bundle
        .subtitles
        .iter()
        .any(|p| !p.valid() || !names.insert(p.name.as_str()))
    {
        return Err("study-presets-invalid");
    }
    Ok(())
}
/// Export only the two named study-filter collections as versioned JSON.
pub fn export(config: &Config) -> Result<Vec<u8>, &'static str> {
    let bundle = Bundle {
        format: 1,
        vocabulary: config.lookup.study_presets.clone(),
        subtitles: config.subtitles.study_presets.clone(),
    };
    validate(&bundle)?;
    let data = serde_json::to_vec_pretty(&bundle).map_err(|_| "study-presets-invalid")?;
    if data.len() > MAX_BYTES {
        return Err("study-presets-too-large");
    }
    Ok(data)
}
/// Validate the entire file and merged capacities before returning a modified configuration.
pub fn import(data: &[u8], current: &Config) -> Result<Config, &'static str> {
    if data.len() > MAX_BYTES {
        return Err("study-presets-too-large");
    }
    let value: serde_json::Value =
        serde_json::from_slice(data).map_err(|_| "study-presets-invalid")?;
    // Unknown filter keys must not silently lose a future client's constraints.
    for (name, template) in [
        ("vocabulary", serde_json::to_value(StudyPreset::default())),
        (
            "subtitles",
            serde_json::to_value(SubtitleStudyPreset::default()),
        ),
    ] {
        let template = template.map_err(|_| "study-presets-invalid")?;
        let allowed = template.as_object().ok_or("study-presets-invalid")?;
        if let Some(rows) = value.get(name) {
            let rows = rows.as_array().ok_or("study-presets-invalid")?;
            if rows.len() > 16 {
                return Err("study-presets-too-large");
            }
            for row in rows {
                if row
                    .as_object()
                    .is_none_or(|r| r.keys().any(|key| !allowed.contains_key(key)))
                {
                    return Err("study-presets-invalid");
                }
            }
        }
    }
    let bundle: Bundle = serde_json::from_value(value).map_err(|_| "study-presets-invalid")?;
    validate(&bundle)?;
    let mut next = current.clone();
    for p in bundle.vocabulary {
        if let Some(old) = next
            .lookup
            .study_presets
            .iter_mut()
            .find(|old| old.name == p.name)
        {
            *old = p;
        } else {
            next.lookup.study_presets.push(p);
        }
    }
    for p in bundle.subtitles {
        if let Some(old) = next
            .subtitles
            .study_presets
            .iter_mut()
            .find(|old| old.name == p.name)
        {
            *old = p;
        } else {
            next.subtitles.study_presets.push(p);
        }
    }
    if next.lookup.study_presets.len() > 16 || next.subtitles.study_presets.len() > 16 {
        return Err("study-presets-too-large");
    }
    Ok(next)
}

#[cfg(test)]
mod tests {
    use super::*;
    // T-KNW-052: only study filters round-trip; merging replaces names and preserves unrelated config/presets.
    #[test]
    fn portable_presets_round_trip_without_exporting_other_configuration() {
        let mut source = Config::default();
        source.paths.ffmpeg = "private-tool-path".into();
        source.lookup.study_presets.push(StudyPreset {
            name: "Cats".into(),
            query: "猫".into(),
            ..Default::default()
        });
        source.subtitles.study_presets.push(SubtitleStudyPreset {
            name: "Exact".into(),
            exact: true,
            order: 2,
            ..Default::default()
        });
        let bytes = export(&source).unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains("private-tool-path"));
        let mut current = Config::default();
        current.lookup.study_presets.push(StudyPreset {
            name: "Other".into(),
            ..Default::default()
        });
        let next = import(&bytes, &current).unwrap();
        assert_eq!(next.paths, current.paths);
        assert_eq!(next.lookup.study_presets.len(), 2);
        assert!(next.subtitles.study_presets[0].exact);
        assert_eq!(import(&bytes, &next).unwrap(), next);
    }
    // T-KNW-052: malformed, unknown, duplicate, invalid and oversized files never change the original config.
    #[test]
    fn invalid_or_over_capacity_imports_are_atomic() {
        let mut current = Config::default();
        for raw in [
            r#"{"format":2}"#,
            r#"{"format":1,"secrets":{}}"#,
            r#"{"format":1,"vocabulary":[{"name":"x","unknown":true}]}"#,
            r#"{"format":1,"vocabulary":[{"name":"x"},{"name":"x"}]}"#,
            r#"{"format":1,"subtitles":[{"name":"x","order":99}]}"#,
        ] {
            assert!(import(raw.as_bytes(), &current).is_err(), "{raw}");
        }
        assert!(import(&vec![b' '; MAX_BYTES + 1], &current).is_err());
        current.lookup.study_presets = (0..16)
            .map(|i| StudyPreset {
                name: i.to_string(),
                ..Default::default()
            })
            .collect();
        let before = current.clone();
        assert!(import(br#"{"format":1,"vocabulary":[{"name":"new"}]}"#, &current).is_err());
        assert_eq!(current, before);
    }
}
