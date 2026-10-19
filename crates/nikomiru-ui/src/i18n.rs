// SPDX-License-Identifier: GPL-3.0-only

//! Interface strings with Fluent (SET-004): `assets/i18n/<locale>/main.ftl`,
//! embedded in the binary. The language comes from the setting or, for
//! `"system"`, from `LC_ALL` / `LC_MESSAGES` / `LANG`, with English as fallback.

use fluent_bundle::concurrent::FluentBundle;
use fluent_bundle::{FluentArgs, FluentResource};
use unic_langid::LanguageIdentifier;

/// Supported interface languages and their Fluent sources.
pub const SOURCES: [(&str, &str); 2] = [
    ("en", include_str!("../../../assets/i18n/en/main.ftl")),
    ("it", include_str!("../../../assets/i18n/it/main.ftl")),
];

/// Translates message ids into the interface language.
pub struct Translator {
    language: &'static str,
    bundle: FluentBundle<FluentResource>,
    fallback: FluentBundle<FluentResource>,
}

impl std::fmt::Debug for Translator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Translator")
            .field("language", &self.language)
            .finish()
    }
}

impl Clone for Translator {
    /// Another translator for the same language (the bundles are rebuilt).
    fn clone(&self) -> Self {
        Self::new(self.language)
    }
}

fn bundle(language: &'static str) -> FluentBundle<FluentResource> {
    let source = SOURCES
        .iter()
        .find(|(l, _)| *l == language)
        .map_or(SOURCES[0].1, |(_, s)| *s);
    // A syntax error keeps the entries that parsed: the tests catch it.
    let resource = FluentResource::try_new(source.to_string()).unwrap_or_else(|(r, _)| r);
    let id: LanguageIdentifier = language.parse().unwrap_or_default();
    let mut b = FluentBundle::new_concurrent(vec![id]);
    b.set_use_isolating(false);
    let _ = b.add_resource(resource);
    b
}

/// The supported language for a locale string like `it_IT.UTF-8`, if any.
fn supported(locale: &str) -> Option<&'static str> {
    let lang = locale
        .split(['_', '.', '@', '-'])
        .next()?
        .to_ascii_lowercase();
    SOURCES.iter().map(|(l, _)| *l).find(|l| *l == lang)
}

impl Translator {
    /// A translator for a supported language (`"en"`, `"it"`); others give English.
    pub fn new(language: &str) -> Self {
        let language = supported(language).unwrap_or("en");
        Self {
            language,
            bundle: bundle(language),
            fallback: bundle("en"),
        }
    }

    /// From the `general.ui_language` setting; `"system"` reads the locale
    /// variables through `var`.
    pub fn from_setting(setting: &str, var: impl Fn(&str) -> Option<String>) -> Self {
        if setting != "system" {
            return Self::new(setting);
        }
        let locale = ["LC_ALL", "LC_MESSAGES", "LANG"]
            .iter()
            .filter_map(|k| var(k))
            .find(|v| !v.is_empty())
            .unwrap_or_default();
        Self::new(&locale)
    }

    /// The language in use.
    pub fn language(&self) -> &'static str {
        self.language
    }

    /// The translated message of an error (ERR-001).
    pub fn error(&self, e: &dyn nikomiru_core::Localized) -> String {
        self.tr(e.message_id(), &e.message_args())
    }

    /// The message `id` in this language only, without the English fallback;
    /// an error if it is missing or an argument it uses is not given (T-ERR-001).
    pub fn try_tr(&self, id: &str, args: &[(&str, String)]) -> Result<String, String> {
        let mut fa = FluentArgs::new();
        for (k, v) in args {
            fa.set(*k, v.clone());
        }
        let pattern = self
            .bundle
            .get_message(id)
            .and_then(|m| m.value())
            .ok_or_else(|| format!("{}: no message {id}", self.language))?;
        let mut errors = Vec::new();
        let text = self.bundle.format_pattern(pattern, Some(&fa), &mut errors);
        if errors.is_empty() {
            Ok(text.into_owned())
        } else {
            Err(format!("{}: {id}: {errors:?}", self.language))
        }
    }

    /// The message `id`, with `args` as `{ $name }` values.
    pub fn tr(&self, id: &str, args: &[(&str, String)]) -> String {
        let mut fa = FluentArgs::new();
        for (k, v) in args {
            fa.set(*k, v.clone());
        }
        for b in [&self.bundle, &self.fallback] {
            if let Some(pattern) = b.get_message(id).and_then(|m| m.value()) {
                let mut errors = Vec::new();
                return b
                    .format_pattern(pattern, Some(&fa), &mut errors)
                    .into_owned();
            }
        }
        id.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};

    fn messages(source: &str) -> BTreeMap<String, BTreeSet<String>> {
        let mut out = BTreeMap::new();
        for line in source.lines() {
            let Some((id, value)) = line.split_once(" = ") else {
                continue;
            };
            if id.starts_with('#') || id.contains(' ') {
                continue;
            }
            let vars = value
                .split("{ $")
                .skip(1)
                .filter_map(|s| s.split(' ').next())
                .map(str::to_string)
                .collect();
            out.insert(id.to_string(), vars);
        }
        out
    }

    // T-I18N-001
    #[test]
    fn every_language_has_the_same_keys_and_arguments() {
        let en = messages(SOURCES[0].1);
        assert!(en.len() > 20);
        for (lang, source) in &SOURCES[1..] {
            assert_eq!(messages(source), en, "{lang} differs from en");
        }
        for (lang, source) in SOURCES {
            assert!(
                FluentResource::try_new(source.to_string()).is_ok(),
                "{lang}: syntax error"
            );
        }
    }

    // SUB-005: every encoding the detection can report has a readable name
    #[test]
    fn every_encoding_has_a_readable_name() {
        let names = ["UTF-8", "UTF-16LE", "UTF-16BE", "ISO-8859-1"]
            .into_iter()
            .chain(nikomiru_subs::encoding::LEGACY);
        for (lang, _) in SOURCES {
            let t = Translator::new(lang);
            for e in names.clone() {
                let id = format!("encoding-{}", e.to_ascii_lowercase());
                assert!(t.try_tr(&id, &[]).is_ok(), "{lang}: {id}");
            }
        }
    }

    // T-SET-004
    #[test]
    fn language_from_the_locale_with_english_fallback() {
        let env = |lang: &'static str| move |k: &str| (k == "LANG").then(|| lang.to_string());
        assert_eq!(
            Translator::from_setting("system", env("it_IT.UTF-8")).language(),
            "it"
        );
        assert_eq!(
            Translator::from_setting("system", env("de_DE.UTF-8")).language(),
            "en"
        );
        assert_eq!(
            Translator::from_setting("it", env("en_US.UTF-8")).language(),
            "it"
        );
        let t = Translator::new("it");
        assert_eq!(
            t.tr("toast-volume", &[("value", "80".into())]),
            "Volume 80%"
        );
        assert_eq!(t.tr("pause", &[]), "Pausa");
        assert_eq!(t.tr("no-such-key", &[]), "no-such-key");
    }
}
