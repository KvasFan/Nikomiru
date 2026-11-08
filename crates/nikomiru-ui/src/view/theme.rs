// SPDX-License-Identifier: GPL-3.0-only

//! Fonts and style. No font is bundled (SPECIFICATION §3.4): the system's are
//! found through fontique (fontconfig), with a Japanese font for Han
//! characters, because subtitles and dictionaries are Japanese whatever the
//! interface language. egui has no synthetic bold, so bold text has its own
//! family ([`bold`]).

use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use egui::epaint::text::{FontData, FontInsert, FontPriority, FontProvider, InsertFontFamily};
use egui::{FontFamily, FontId, TextStyle};
use fontique::{
    Attributes, Collection, CollectionOptions, FallbackKey, FontWeight, GenericFamily, Language,
    QueryFamily, QueryFont, QueryStatus, Script, SourceCache, SourceCacheOptions,
};

const BOLD: &str = "bold";

/// The prefix of the families of the subtitles' font (SUB-024), regular and
/// bold: the system family's name follows.
const SUBTITLE: &str = "subtitle:";
const SUBTITLE_BOLD: &str = "subtitle-bold:";

/// The family of bold text.
pub(crate) fn bold() -> FontFamily {
    FontFamily::Name(BOLD.into())
}

/// The family of subtitle text in the system family `font` (SUB-024): the
/// interface's when `font` is empty.
pub(crate) fn subtitle_family(font: &str, bold_text: bool) -> FontFamily {
    match (font.is_empty(), bold_text) {
        (true, false) => FontFamily::Proportional,
        (true, true) => bold(),
        (false, false) => FontFamily::Name(format!("{SUBTITLE}{font}").into()),
        (false, true) => FontFamily::Name(format!("{SUBTITLE_BOLD}{font}").into()),
    }
}

/// The system's font families, once read on a thread of their own
/// (SUB-024): all of them by their name in lower case, and the ones with
/// Japanese characters as they are named.
struct Families {
    all: std::collections::HashSet<String>,
    japanese: Vec<String>,
}

static FAMILIES: std::sync::OnceLock<Families> = std::sync::OnceLock::new();
static READING_FAMILIES: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The families, or `None` while they are being read (the first call starts).
fn families() -> Option<&'static Families> {
    if let Some(families) = FAMILIES.get() {
        return Some(families);
    }
    if !READING_FAMILIES.swap(true, std::sync::atomic::Ordering::SeqCst) {
        let spawned = std::thread::Builder::new()
            .name("font-families".into())
            .spawn(|| {
                let _ = FAMILIES.set(read_families());
            });
        if let Err(e) = spawned {
            tracing::warn!("cannot list the fonts: {e}");
        }
    }
    None
}

fn read_families() -> Families {
    let mut collection = Collection::new(CollectionOptions {
        shared: false,
        system_fonts: true,
    });
    let mut cache = SourceCache::new(SourceCacheOptions::default());
    let names: Vec<String> = collection.family_names().map(str::to_string).collect();
    let mut japanese: Vec<String> = names
        .iter()
        .filter(|name| {
            let mut query = collection.query(&mut cache);
            query.set_families([QueryFamily::Named(name.as_str())]);
            let mut has = false;
            query.matches_with(|font| {
                has = font.charmap().is_some_and(|map| {
                    ['日', 'あ', 'ア']
                        .iter()
                        .all(|c| map.map(u32::from(*c)).is_some())
                });
                QueryStatus::Stop
            });
            has
        })
        .cloned()
        .collect();
    japanese.sort_by_key(|name| name.to_lowercase());
    japanese.dedup();
    Families {
        all: names.iter().map(|n| n.to_lowercase()).collect(),
        japanese,
    }
}

/// The system families with Japanese characters, by name; `None` while they
/// are being read (SUB-024).
pub(crate) fn japanese_families() -> Option<&'static [String]> {
    families().map(|f| f.japanese.as_slice())
}

/// The system has the family `name`; not known yet counts as no (SUB-024).
pub(crate) fn installed(name: &str) -> bool {
    families().is_some_and(|f| f.all.contains(&name.to_lowercase()))
}

/// The face of the system family `name` with `weight`, if installed.
fn named(
    collection: &mut Collection,
    cache: &mut SourceCache,
    name: &str,
    weight: FontWeight,
) -> Option<QueryFont> {
    let mut query = collection.query(cache);
    query.set_families([QueryFamily::Named(name)]);
    query.set_attributes(Attributes {
        weight,
        ..Attributes::default()
    });
    let mut found = None;
    query.matches_with(|font| {
        found = Some(font.clone());
        QueryStatus::Stop
    });
    found
}

/// Gives `ctx` the system fonts and the style.
pub(crate) fn install(ctx: &egui::Context) {
    ctx.add_font_provider(Arc::new(SystemFonts::new()));
    // symbols and scripts the fonts above lack (▾, ↻, emoji…)
    ctx.add_font_provider(Arc::new(
        egui_system_font_provider::SystemFontProvider::new().with_locale(Some("ja")),
    ));
    // dark until the settings say otherwise (SET-003)
    super::palette::apply(ctx, true, super::palette::ACCENT);
    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Small, FontId::proportional(12.0)),
            (TextStyle::Body, FontId::proportional(15.0)),
            (TextStyle::Button, FontId::proportional(15.0)),
            (TextStyle::Heading, FontId::new(22.0, bold())),
            (TextStyle::Monospace, FontId::monospace(14.0)),
        ]
        .into();
        // tooltips after 300 ms
        style.interaction.tooltip_delay = 0.3;
        // a drag over text is not a selection: clicks on words look them up
        style.interaction.selectable_labels = false;
    });
}

/// The system's sans-serif and Japanese fonts, regular and bold: the head of
/// the proportional family and of [`bold`]. Enumerating the system fonts takes
/// a while, so it starts on a thread at once.
struct SystemFonts {
    loading: Mutex<Option<JoinHandle<Collection>>>,
    loaded: Mutex<Option<(Collection, SourceCache)>>,
}

impl SystemFonts {
    fn new() -> Self {
        let loading = std::thread::Builder::new()
            .name("fonts".into())
            .spawn(|| {
                Collection::new(CollectionOptions {
                    shared: false,
                    system_fonts: true,
                })
            })
            .ok();
        Self {
            loading: Mutex::new(loading),
            loaded: Mutex::new(None),
        }
    }

    fn with_collection<T>(&self, f: impl FnOnce(&mut Collection, &mut SourceCache) -> T) -> T {
        let mut loaded = self
            .loaded
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let (collection, cache) = loaded.get_or_insert_with(|| {
            let thread = self
                .loading
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take();
            let collection = thread.and_then(|t| t.join().ok()).unwrap_or_else(|| {
                Collection::new(CollectionOptions {
                    shared: false,
                    system_fonts: true,
                })
            });
            (collection, SourceCache::new(SourceCacheOptions::default()))
        });
        f(collection, cache)
    }
}

/// The first font of `generic` with `weight` that has `probe`; for Han, the
/// Japanese fallback of the script.
fn find(
    collection: &mut Collection,
    cache: &mut SourceCache,
    generic: GenericFamily,
    weight: FontWeight,
    han: bool,
) -> Option<QueryFont> {
    let probe = if han { '日' } else { 'a' };
    let mut query = collection.query(cache);
    query.set_families([QueryFamily::Generic(generic)]);
    query.set_attributes(Attributes {
        weight,
        ..Attributes::default()
    });
    if han {
        let japanese = Language::parse("ja").ok();
        query.set_fallbacks(FallbackKey::new(
            Script::from_bytes(*b"Hani"),
            japanese.as_ref(),
        ));
    }
    let mut found = None;
    query.matches_with(|font| {
        if font
            .charmap()
            .is_some_and(|map| map.map(u32::from(probe)).is_some())
        {
            found = Some(font.clone());
            QueryStatus::Stop
        } else {
            QueryStatus::Continue
        }
    });
    found
}

impl FontProvider for SystemFonts {
    fn fonts_for_family(&self, family: &FontFamily) -> Vec<FontInsert> {
        // SUB-024: the family chosen first, the interface's after it
        let subtitle = match family {
            FontFamily::Name(name) => name
                .strip_prefix(SUBTITLE_BOLD)
                .map(|n| (n, FontWeight::BOLD))
                .or_else(|| name.strip_prefix(SUBTITLE).map(|n| (n, FontWeight::NORMAL))),
            _ => None,
        };
        let (subtitle, family) = match subtitle {
            Some((name, weight)) => {
                let own = self.with_collection(|collection, cache| {
                    named(collection, cache, name, weight)
                        .map(|font| insert(collection, font, family))
                });
                let interface = if weight == FontWeight::BOLD {
                    bold()
                } else {
                    FontFamily::Proportional
                };
                (Some((own, family.clone())), interface)
            }
            None => (None, family.clone()),
        };
        let mut fonts = self.interface_fonts(&family);
        if let Some((own, asked)) = subtitle {
            for font in &mut fonts {
                font.families = vec![InsertFontFamily {
                    family: asked.clone(),
                    priority: FontPriority::Lowest,
                }];
            }
            fonts.splice(0..0, own);
        }
        fonts
    }
}

/// A font of the collection as egui installs it, for `family`.
fn insert(collection: &mut Collection, font: QueryFont, family: &FontFamily) -> FontInsert {
    let family_name = collection.family_name(font.family.0).unwrap_or("system");
    let name = format!("{family_name}:{}:{}", font.family.1, font.index);
    tracing::debug!("font for {family:?}: {name}");
    let (blob, _) = font.blob.into_raw_parts();
    FontInsert {
        name,
        data: FontData::from_blob(blob, font.index),
        families: vec![InsertFontFamily {
            family: family.clone(),
            priority: FontPriority::Lowest,
        }],
    }
}

impl SystemFonts {
    /// The interface's fonts of `family`: its sans-serif or monospace, then a
    /// Japanese font for Han.
    fn interface_fonts(&self, family: &FontFamily) -> Vec<FontInsert> {
        let (generic, weight, scripts): (_, _, &[bool]) = match family {
            FontFamily::Proportional => {
                (GenericFamily::SansSerif, FontWeight::NORMAL, &[false, true])
            }
            FontFamily::Monospace => (GenericFamily::Monospace, FontWeight::NORMAL, &[false, true]),
            FontFamily::Name(name) if &**name == BOLD => {
                (GenericFamily::SansSerif, FontWeight::BOLD, &[false, true])
            }
            FontFamily::Name(_) => return Vec::new(),
        };
        self.with_collection(|collection, cache| {
            let fonts: Vec<QueryFont> = scripts
                .iter()
                .filter_map(|&han| find(collection, cache, generic, weight, han))
                .collect();
            fonts
                .into_iter()
                .map(|font| insert(collection, font, family))
                .collect()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(fonts: &[FontInsert]) -> Vec<String> {
        fonts.iter().map(|f| f.name.clone()).collect()
    }

    // T-SUB-024: the family chosen first, the interface's fonts after it;
    // nothing of its own for a family not installed
    #[test]
    fn a_subtitle_font_comes_before_the_interfaces() {
        let fonts = SystemFonts::new();
        let interface = names(&fonts.fonts_for_family(&FontFamily::Proportional));
        let missing = subtitle_family("No Such Font 123", false);
        let got = fonts.fonts_for_family(&missing);
        assert_eq!(names(&got), interface);
        assert!(got.iter().all(|f| f.families[0].family == missing));
        // a Japanese family the interface does not use already, if there is one
        let Some(japanese) = read_families().japanese.into_iter().find(|family| {
            !interface
                .iter()
                .any(|n| n.starts_with(&format!("{family}:")))
        }) else {
            eprintln!("no other Japanese font on this system");
            return;
        };
        let got = names(&fonts.fonts_for_family(&subtitle_family(&japanese, false)));
        assert!(got[0].starts_with(&format!("{japanese}:")), "{got:?}");
        assert_eq!(got[1..], interface[..]);
        assert_eq!(
            subtitle_family("", true),
            bold(),
            "empty: the interface's own"
        );
    }
}
