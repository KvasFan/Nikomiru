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

/// The family of bold text.
pub(crate) fn bold() -> FontFamily {
    FontFamily::Name(BOLD.into())
}

/// Gives `ctx` the system fonts and the style.
pub(crate) fn install(ctx: &egui::Context) {
    ctx.add_font_provider(Arc::new(SystemFonts::new()));
    // symbols and scripts the fonts above lack (▾, ↻, emoji…)
    ctx.add_font_provider(Arc::new(
        egui_system_font_provider::SystemFontProvider::new().with_locale(Some("ja")),
    ));
    // a video player: always dark, whatever the desktop's preference
    ctx.set_theme(egui::Theme::Dark);
    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Small, FontId::proportional(12.0)),
            (TextStyle::Body, FontId::proportional(15.0)),
            (TextStyle::Button, FontId::proportional(15.0)),
            (TextStyle::Heading, FontId::new(22.0, bold())),
            (TextStyle::Monospace, FontId::monospace(14.0)),
        ]
        .into();
        // QML's tooltips appeared after 300 ms
        style.interaction.tooltip_delay = 0.3;
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
                .map(|font| {
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
                })
                .collect()
        })
    }
}
