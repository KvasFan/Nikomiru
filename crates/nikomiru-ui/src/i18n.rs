// SPDX-License-Identifier: GPL-3.0-only

//! Interface strings with Fluent (SET-004): `assets/i18n/<locale>/main.ftl`,
//! embedded in the binary. The language comes from the setting or, for
//! `"system"`, from `LC_ALL` / `LC_MESSAGES` / `LANG`, with English as fallback.
//! The clones of a translator share its language: the settings change it for
//! the interface and the workers at once (SET-002).

use std::sync::{Arc, PoisonError, RwLock};

use fluent_bundle::concurrent::FluentBundle;
use fluent_bundle::{FluentArgs, FluentResource, FluentValue};
use unic_langid::LanguageIdentifier;

/// Supported interface languages and their Fluent sources.
pub const SOURCES: [(&str, &str); 2] = [
    ("en", include_str!("../../../assets/i18n/en/main.ftl")),
    ("it", include_str!("../../../assets/i18n/it/main.ftl")),
];

/// Translates message ids into the interface language.
#[derive(Clone)]
pub struct Translator {
    shared: Arc<RwLock<Bundles>>,
}

struct Bundles {
    language: &'static str,
    bundle: FluentBundle<FluentResource>,
    fallback: FluentBundle<FluentResource>,
}

impl Bundles {
    fn new(language: &'static str) -> Self {
        Self {
            language,
            bundle: bundle(language),
            fallback: bundle("en"),
        }
    }
}

impl std::fmt::Debug for Translator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Translator")
            .field("language", &self.language())
            .finish()
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

/// The language of the setting `"it"`, `"en"`, or for `"system"` the
/// locale of the first of `LC_ALL`, `LC_MESSAGES`, `LANG` that is set.
fn language_of(setting: &str, var: impl Fn(&str) -> Option<String>) -> String {
    if setting != "system" {
        return setting.to_string();
    }
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|k| var(k))
        .find(|v| !v.is_empty())
        .unwrap_or_default()
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
            shared: Arc::new(RwLock::new(Bundles::new(language))),
        }
    }

    /// From the `general.ui_language` setting; `"system"` reads the locale
    /// variables through `var`.
    pub fn from_setting(setting: &str, var: impl Fn(&str) -> Option<String>) -> Self {
        Self::new(&language_of(setting, var))
    }

    /// Changes the language of this translator and of its clones.
    pub fn set_setting(&self, setting: &str, var: impl Fn(&str) -> Option<String>) {
        let language = supported(&language_of(setting, var)).unwrap_or("en");
        if language != self.language() {
            *self.shared.write().unwrap_or_else(PoisonError::into_inner) = Bundles::new(language);
        }
    }

    /// The language in use.
    pub fn language(&self) -> &'static str {
        self.read().language
    }

    fn read(&self) -> std::sync::RwLockReadGuard<'_, Bundles> {
        self.shared.read().unwrap_or_else(PoisonError::into_inner)
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
            fa.set(*k, value(v));
        }
        let b = self.read();
        let pattern = b
            .bundle
            .get_message(id)
            .and_then(|m| m.value())
            .ok_or_else(|| format!("{}: no message {id}", b.language))?;
        let mut errors = Vec::new();
        let text = b.bundle.format_pattern(pattern, Some(&fa), &mut errors);
        if errors.is_empty() {
            Ok(text.into_owned())
        } else {
            Err(format!("{}: {id}: {errors:?}", b.language))
        }
    }

    /// The message `id`, with `args` as `{ $name }` values.
    pub fn tr(&self, id: &str, args: &[(&str, String)]) -> String {
        let mut fa = FluentArgs::new();
        for (k, v) in args {
            fa.set(*k, value(v));
        }
        let bundles = self.read();
        for b in [&bundles.bundle, &bundles.fallback] {
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

/// An argument for Fluent: a plain integer as a number, so a message can
/// choose its plural (`[one]`); anything else, `07` or a date included, as
/// text.
fn value(text: &str) -> FluentValue<'static> {
    let integer = !text.is_empty()
        && text.chars().all(|c| c.is_ascii_digit())
        && (text == "0" || !text.starts_with('0'));
    match text.parse::<i64>() {
        Ok(n) if integer => FluentValue::from(n),
        _ => FluentValue::from(text.to_string()),
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
        // the settings change the language of every clone (SET-002)
        let worker = t.clone();
        t.set_setting("en", |_| None);
        assert_eq!(worker.tr("pause", &[]), "Pause");
        t.set_setting("system", env("it_IT.UTF-8"));
        assert_eq!(worker.language(), "it");
    }

    // plurals choose by number; other values stay text
    #[test]
    fn integers_choose_their_plural() {
        let t = Translator::from_setting("en", |_| None);
        assert_eq!(
            t.tr(
                "stats-session",
                &[
                    ("time", "5 min".into()),
                    ("chars", "16".into()),
                    ("lookups", "1".into()),
                    ("cards", "2".into())
                ]
            ),
            "5 min · 16 characters · 1 lookup · 2 cards"
        );
        let t = Translator::from_setting("it", |_| None);
        assert!(
            t.tr(
                "stats-session",
                &[
                    ("time", "5 min".into()),
                    ("chars", "1".into()),
                    ("lookups", "2".into()),
                    ("cards", "1".into())
                ]
            )
            .ends_with("2 ricerche · 1 card")
        );
        assert!(matches!(value("07"), FluentValue::String(_)));
        assert!(matches!(value("2026-10-25"), FluentValue::String(_)));
        assert!(matches!(value("1234"), FluentValue::Number(_)));
    }
}
