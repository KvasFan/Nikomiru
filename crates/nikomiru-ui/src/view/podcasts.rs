// SPDX-License-Identifier: GPL-3.0-only

//! The Podcasts page (POD-001…POD-004): feeds added by address, their
//! episodes, explicit downloads and timed text.

use crate::podcasts::State;
use crate::{session::Session, ui::Command};
use nikomiru_library::podcasts::{Episode, Feed};

struct Scene<'a> {
    state: &'a State,
    writable: bool,
    /// Whether an episode has a partial download to resume.
    partial: &'a dyn Fn(&Feed, &Episode) -> bool,
    tr: &'a Translate<'a>,
}

type Translate<'a> = dyn Fn(&str, &[(&str, String)]) -> String + 'a;

pub(super) fn show(ui: &mut egui::Ui, s: &Session, url: &mut String, out: &mut Vec<Command>) {
    let tr = |key: &str, args: &[(&str, String)]| s.translator().tr(key, args);
    let partial = |f: &Feed, e: &Episode| e.file.is_none() && s.podcast_partial(&f.id, e);
    draw(
        ui,
        &Scene {
            state: &s.podcasts,
            writable: s.phrases_writable(),
            partial: &partial,
            tr: &tr,
        },
        url,
        out,
    );
}

fn mib(bytes: u64) -> String {
    format!("{:.1}", bytes as f64 / f64::from(1 << 20))
}

fn draw(ui: &mut egui::Ui, scene: &Scene<'_>, url: &mut String, out: &mut Vec<Command>) {
    let tr = |key: &str| (scene.tr)(key, &[]);
    let state = scene.state;
    ui.horizontal_wrapped(|ui| {
        ui.heading(tr("podcasts-title"));
        if ui.button(tr("podcasts-close")).clicked() {
            out.push(Command::PodcastsClose);
        }
        if state.busy {
            ui.spinner();
        }
    });
    ui.label(tr("podcasts-instructions"));
    ui.horizontal_wrapped(|ui| {
        let label = ui.label(tr("podcasts-url"));
        ui.add(egui::TextEdit::singleline(url).desired_width(420.0))
            .labelled_by(label.id);
        if ui
            .add_enabled(
                scene.writable && state.fetching.is_none() && !url.trim().is_empty(),
                egui::Button::new(tr("podcasts-add")),
            )
            .clicked()
        {
            out.push(Command::PodcastFetch(url.trim().to_string()));
        }
        if state.fetching.is_some() {
            ui.spinner();
            ui.label(tr("podcasts-fetching"));
            if ui.button(tr("podcasts-stop")).clicked() {
                out.push(Command::PodcastFetchCancel);
            }
        }
    });
    if let Some(d) = &state.download {
        ui.horizontal_wrapped(|ui| {
            ui.spinner();
            let total = d.total.map_or_else(|| "?".into(), mib);
            ui.label((scene.tr)(
                "podcasts-downloading",
                &[("done", mib(d.done)), ("total", total)],
            ));
            if ui.button(tr("podcasts-stop")).clicked() {
                out.push(Command::PodcastDownloadCancel);
            }
        });
    }
    if let Some(error) = &state.error {
        ui.colored_label(super::palette::current().bad, error);
    }
    if let Some(notice) = &state.notice {
        ui.label(notice);
    }
    if state.feeds.is_empty() {
        ui.label(tr("podcasts-empty"));
    }
    egui::ScrollArea::vertical()
        .id_salt("podcast-feeds")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for feed in &state.feeds {
                ui.push_id(&feed.id, |ui| feed_rows(ui, scene, feed, out));
            }
        });
}

fn feed_rows(ui: &mut egui::Ui, scene: &Scene<'_>, feed: &Feed, out: &mut Vec<Command>) {
    let tr = |key: &str| (scene.tr)(key, &[]);
    let state = scene.state;
    ui.separator();
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new(&feed.title).strong().size(18.0));
        if ui
            .add_enabled(
                scene.writable && state.fetching.is_none(),
                egui::Button::new(tr("podcasts-refresh")),
            )
            .clicked()
        {
            out.push(Command::PodcastFetch(feed.url.clone()));
        }
        if !feed.link.is_empty() && ui.button(tr("podcasts-feed-page")).clicked() {
            out.push(Command::PodcastLink(feed.link.clone()));
        }
        if ui
            .add_enabled(
                scene.writable && state.download.is_none(),
                egui::Button::new(tr("podcasts-remove")),
            )
            .clicked()
        {
            out.push(Command::PodcastRemove(feed.id.clone()));
        }
    });
    for e in &feed.episodes {
        ui.push_id(&e.id, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(&e.title);
                if !e.published.is_empty() {
                    ui.label(
                        egui::RichText::new(&e.published).color(super::palette::current().details),
                    );
                }
                if !e.listed {
                    ui.label(tr("podcasts-unlisted"));
                }
                let downloading = state
                    .download
                    .as_ref()
                    .is_some_and(|d| d.feed == feed.id && d.episode == e.id);
                if e.file.is_some() {
                    if ui.button(tr("podcasts-play")).clicked() {
                        out.push(Command::PodcastPlay(feed.id.clone(), e.id.clone()));
                    }
                    if ui
                        .add_enabled(scene.writable, egui::Button::new(tr("podcasts-delete")))
                        .clicked()
                    {
                        out.push(Command::PodcastDelete(feed.id.clone(), e.id.clone()));
                    }
                    if ui
                        .add_enabled(scene.writable, egui::Button::new(tr("podcasts-import")))
                        .clicked()
                    {
                        out.push(Command::PodcastImport(feed.id.clone(), e.id.clone()));
                    }
                    ui.label(tr(if e.transcript_file.is_some() {
                        "podcasts-has-text"
                    } else {
                        "podcasts-no-text"
                    }));
                } else if !downloading {
                    let resume = (scene.partial)(feed, e);
                    let label = if resume {
                        "podcasts-resume"
                    } else {
                        "podcasts-download"
                    };
                    if ui
                        .add_enabled(
                            scene.writable && state.download.is_none() && e.listed,
                            egui::Button::new(tr(label)),
                        )
                        .clicked()
                    {
                        out.push(Command::PodcastDownload(feed.id.clone(), e.id.clone()));
                    }
                }
                if !e.link.is_empty() && ui.button(tr("podcasts-episode-page")).clicked() {
                    out.push(Command::PodcastLink(e.link.clone()));
                }
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};

    #[derive(Default)]
    struct Model {
        state: State,
        url: String,
        out: Vec<Command>,
    }

    fn feed() -> Feed {
        Feed {
            id: "f".repeat(32),
            url: "https://x/feed".into(),
            title: "Feed".into(),
            link: "https://x/".into(),
            refreshed_ms: 0,
            episodes: vec![
                Episode {
                    id: "a".repeat(32),
                    title: "Downloaded".into(),
                    file: Some(format!("{}/{}.mp3", "f".repeat(32), "a".repeat(32))),
                    listed: true,
                    ..Episode::default()
                },
                Episode {
                    id: "b".repeat(32),
                    title: "Partial".into(),
                    listed: true,
                    ..Episode::default()
                },
            ],
        }
    }

    // T-POD-004: explicit actions by name; a partial download offers Resume.
    #[test]
    fn the_page_acts_only_on_clicks() {
        let mut h = Harness::builder()
            .with_size(egui::vec2(1000.0, 700.0))
            .allow_missing_glyphs()
            .build_ui_state(
                |ui, m: &mut Model| {
                    draw(
                        ui,
                        &Scene {
                            state: &m.state,
                            writable: true,
                            partial: &|_, e| e.title == "Partial",
                            tr: &|k, _| k.to_owned(),
                        },
                        &mut m.url,
                        &mut m.out,
                    );
                },
                Model {
                    state: State {
                        feeds: vec![feed()],
                        ..State::default()
                    },
                    ..Model::default()
                },
            );
        h.run();
        assert!(h.state().out.is_empty());
        h.get_by_label("podcasts-play").click();
        h.get_by_label("podcasts-resume").click();
        h.get_by_label("podcasts-feed-page").click();
        h.run();
        let out = &h.state().out;
        assert!(
            out.iter()
                .any(|c| matches!(c, Command::PodcastPlay(_, e) if e == &"a".repeat(32)))
        );
        assert!(
            out.iter()
                .any(|c| matches!(c, Command::PodcastDownload(_, e) if e == &"b".repeat(32)))
        );
        assert!(
            out.iter()
                .any(|c| matches!(c, Command::PodcastLink(u) if u == "https://x/"))
        );
        h.get_by_label("podcasts-no-text");
    }
}
