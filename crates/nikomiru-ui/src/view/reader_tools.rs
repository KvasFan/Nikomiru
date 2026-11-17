// SPDX-License-Identifier: GPL-3.0-only

//! The reader tools page (RDR-001…RDR-003): annotations of the books, with
//! their editor, and the search through the text of every book.

use crate::reader_tools::{Scope, State, Tab, matches};
use crate::{session::Session, ui::Command};
use nikomiru_library::annotations::{Annotation, Kind};

/// What the page keeps between frames.
#[derive(Debug, Default)]
pub(crate) struct Query {
    /// The filter over the annotations.
    pub filter: String,
    /// The search being typed.
    pub search: String,
}

struct Scene<'a> {
    state: &'a State,
    writable: bool,
    /// The open text book's uid, and whether each annotation of it is current.
    open: Option<&'a str>,
    current: &'a dyn Fn(&Annotation) -> Option<bool>,
    tr: &'a Translate<'a>,
}

type Translate<'a> = dyn Fn(&str, &[(&str, String)]) -> String + 'a;

pub(super) fn show(ui: &mut egui::Ui, s: &Session, query: &mut Query, out: &mut Vec<Command>) {
    let tr = |key: &str, args: &[(&str, String)]| s.translator().tr(key, args);
    let current = |a: &Annotation| s.annotation_current(a);
    let open = s
        .books_page
        .open
        .as_ref()
        .filter(|o| !o.book.manga)
        .map(|o| o.uid.as_str());
    draw(
        ui,
        &Scene {
            state: &s.reader,
            writable: s.phrases_writable(),
            open,
            current: &current,
            tr: &tr,
        },
        query,
        out,
    );
}

fn draw(ui: &mut egui::Ui, scene: &Scene<'_>, query: &mut Query, out: &mut Vec<Command>) {
    let tr = |key: &str| (scene.tr)(key, &[]);
    let state = scene.state;
    ui.horizontal_wrapped(|ui| {
        ui.heading(tr("reader-tools-title"));
        if ui.button(tr("reader-tools-close")).clicked() {
            out.push(Command::ReaderToolsClose);
        }
    });
    ui.horizontal(|ui| {
        if ui
            .selectable_label(state.tab == Tab::Annotations, tr("annotations-title"))
            .clicked()
        {
            out.push(Command::ReaderToolsOpen(Tab::Annotations));
        }
        if ui
            .selectable_label(state.tab == Tab::Search, tr("search-title"))
            .clicked()
        {
            out.push(Command::ReaderToolsOpen(Tab::Search));
        }
    });
    ui.separator();
    match state.tab {
        Tab::Annotations => annotations(ui, scene, &mut query.filter, out),
        Tab::Search => search(ui, scene, &mut query.search, out),
    }
}

fn kind_label(kind: Kind) -> &'static str {
    match kind {
        Kind::Bookmark => "annotation-kind-bookmark",
        Kind::Highlight => "annotation-kind-highlight",
        Kind::Note => "annotation-kind-note",
    }
}

/// The button that starts a new annotation of `kind`.
pub(super) fn new_label(kind: Kind) -> &'static str {
    match kind {
        Kind::Bookmark => "annotation-new-bookmark",
        Kind::Highlight => "annotation-new-highlight",
        Kind::Note => "annotation-new-note",
    }
}

fn annotations(ui: &mut egui::Ui, scene: &Scene<'_>, filter: &mut String, out: &mut Vec<Command>) {
    let tr = |key: &str| (scene.tr)(key, &[]);
    let state = scene.state;
    if state.busy {
        ui.spinner();
    }
    if let Some(error) = &state.error {
        ui.colored_label(super::palette::current().bad, error);
    }
    if !scene.writable {
        ui.label(tr("phrases-readonly"));
    }
    ui.add_enabled_ui(!state.busy, |ui| {
        ui.horizontal_wrapped(|ui| {
            for kind in [Kind::Bookmark, Kind::Highlight, Kind::Note] {
                if ui
                    .add_enabled(
                        scene.open.is_some() && scene.writable && state.editor.is_none(),
                        egui::Button::new(tr(new_label(kind))),
                    )
                    .on_disabled_hover_text(tr("annotations-no-book"))
                    .clicked()
                {
                    out.push(Command::AnnotationNew(kind));
                }
            }
        });
        if let Some(original) = &state.editor {
            editor(ui, scene, original, out);
            return;
        }
        ui.horizontal_wrapped(|ui| {
            let label = ui.label(tr("annotations-search"));
            ui.add(egui::TextEdit::singleline(filter).desired_width(260.0))
                .labelled_by(label.id);
            if scene.open.is_some()
                && ui
                    .checkbox(&mut state.this_book.clone(), tr("annotations-this-book"))
                    .clicked()
            {
                out.push(Command::AnnotationsThisBook(!state.this_book));
            }
        });
        let rows: Vec<&Annotation> = state
            .items
            .iter()
            .filter(|a| !state.this_book || Some(a.book.as_str()) == scene.open)
            .filter(|a| matches(a, filter))
            .collect();
        ui.horizontal(|ui| {
            ui.label(format!("{} / {}", rows.len(), state.items.len()));
            if ui
                .add_enabled(
                    !rows.is_empty(),
                    egui::Button::new(tr("annotations-export")),
                )
                .clicked()
            {
                out.push(Command::AnnotationExport(
                    rows.iter().map(|a| a.id.clone()).collect(),
                ));
            }
        });
        egui::ScrollArea::vertical()
            .id_salt("annotation-rows")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for a in rows {
                    ui.push_id(&a.id, |ui| {
                        ui.separator();
                        let stale = (scene.current)(a) == Some(false);
                        ui.horizontal_wrapped(|ui| {
                            ui.label(egui::RichText::new(tr(kind_label(a.kind))).strong());
                            if !a.name.is_empty() {
                                ui.label(egui::RichText::new(&a.name).strong());
                            }
                            ui.label(
                                egui::RichText::new(format!(
                                    "{} · {} {}",
                                    a.title,
                                    tr("annotations-chapter"),
                                    a.chapter + 1
                                ))
                                .color(super::palette::current().details),
                            );
                            if stale {
                                ui.colored_label(
                                    super::palette::current().bad,
                                    tr("annotations-stale"),
                                );
                            }
                        });
                        ui.label(&a.quote);
                        if !a.note.is_empty() {
                            ui.label(egui::RichText::new(&a.note).italics());
                        }
                        ui.horizontal_wrapped(|ui| {
                            if ui
                                .add_enabled(!stale, egui::Button::new(tr("annotations-go")))
                                .clicked()
                            {
                                out.push(Command::AnnotationGo(a.id.clone()));
                            }
                            if ui
                                .add_enabled(scene.writable, egui::Button::new(tr("phrases-edit")))
                                .clicked()
                            {
                                out.push(Command::AnnotationEdit(Some(a.clone())));
                            }
                            if ui
                                .add_enabled(
                                    scene.writable,
                                    egui::Button::new(tr("phrases-delete")),
                                )
                                .clicked()
                            {
                                out.push(Command::AnnotationDelete(a.id.clone()));
                            }
                        });
                    });
                }
            });
    });
}

fn editor(ui: &mut egui::Ui, scene: &Scene<'_>, original: &Annotation, out: &mut Vec<Command>) {
    let tr = |key: &str| (scene.tr)(key, &[]);
    let mut a = original.clone();
    ui.label(egui::RichText::new(tr(kind_label(a.kind))).strong());
    ui.label(egui::RichText::new(&a.title).color(super::palette::current().details));
    let fresh = !scene.state.items.iter().any(|i| i.id == a.id);
    if fresh && a.kind != Kind::Bookmark {
        ui.horizontal_wrapped(|ui| {
            ui.label(tr("annotations-cover"));
            for (scope, key) in [
                (Scope::Word, "annotations-cover-word"),
                (Scope::Sentence, "annotations-cover-sentence"),
                (Scope::Paragraph, "annotations-cover-paragraph"),
            ] {
                if ui.button(tr(key)).clicked() {
                    out.push(Command::AnnotationScope(scope));
                }
            }
        });
    }
    let label = ui.label(tr("annotations-quote"));
    ui.add(
        egui::Label::new(egui::RichText::new(&a.quote).size(18.0))
            .wrap()
            .sense(egui::Sense::hover()),
    )
    .labelled_by(label.id);
    let label = ui.label(tr("annotations-name"));
    ui.add(
        egui::TextEdit::singleline(&mut a.name)
            .id_salt("annotation-name")
            .desired_width(f32::INFINITY),
    )
    .labelled_by(label.id);
    let label = ui.label(tr("annotations-note"));
    ui.add(
        egui::TextEdit::multiline(&mut a.note)
            .id_salt("annotation-note")
            .desired_rows(4)
            .desired_width(f32::INFINITY),
    )
    .labelled_by(label.id);
    let valid = a.valid();
    if &a != original {
        out.push(Command::AnnotationEdit(Some(a)));
    }
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                valid && scene.writable,
                egui::Button::new(tr("phrases-save")),
            )
            .clicked()
        {
            out.push(Command::AnnotationSave);
        }
        if ui.button(tr("phrases-cancel")).clicked() {
            out.push(Command::AnnotationEdit(None));
        }
    });
    if !valid {
        ui.label(tr("annotations-invalid"));
    }
}

fn search(ui: &mut egui::Ui, scene: &Scene<'_>, typed: &mut String, out: &mut Vec<Command>) {
    let tr = |key: &str| (scene.tr)(key, &[]);
    let search = &scene.state.search;
    ui.label(tr("search-instructions"));
    let mut go = false;
    ui.horizontal_wrapped(|ui| {
        let label = ui.label(tr("search-query"));
        let field = ui
            .add(egui::TextEdit::singleline(typed).desired_width(320.0))
            .labelled_by(label.id);
        go = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let valid = nikomiru_text::literal::Needle::new(typed).is_some();
        go |= ui
            .add_enabled(valid, egui::Button::new(tr("search-run")))
            .clicked();
        if search.running {
            ui.spinner();
            if ui.button(tr("search-cancel")).clicked() {
                out.push(Command::SearchCancel);
            }
        }
    });
    if go {
        out.push(Command::SearchRun(typed.clone()));
    }
    if let Some(error) = &search.error {
        ui.colored_label(super::palette::current().bad, error);
    }
    let Some(outcome) = &search.outcome else {
        return;
    };
    ui.label((scene.tr)(
        "search-summary",
        &[
            ("hits", outcome.hits.len().to_string()),
            ("books", outcome.searched.to_string()),
        ],
    ));
    if outcome.truncated {
        ui.label(tr("search-truncated"));
    }
    if !outcome.failed.is_empty() {
        ui.colored_label(
            super::palette::current().bad,
            (scene.tr)("search-unreadable", &[("books", outcome.failed.join(", "))]),
        );
    }
    egui::ScrollArea::vertical()
        .id_salt("search-hits")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for (index, hit) in outcome.hits.iter().enumerate() {
                ui.push_id(index, |ui| {
                    ui.separator();
                    ui.label(
                        egui::RichText::new(format!(
                            "{} · {} {}",
                            hit.title,
                            tr("annotations-chapter"),
                            hit.chapter + 1
                        ))
                        .color(super::palette::current().details),
                    );
                    let (text, range) = &hit.snippet;
                    let mut job = egui::text::LayoutJob::default();
                    let plain = egui::TextFormat {
                        color: super::palette::current().text,
                        ..Default::default()
                    };
                    let strong = egui::TextFormat {
                        background: super::books_page::MARK,
                        ..plain.clone()
                    };
                    job.append(&text[..range.start], 0.0, plain.clone());
                    job.append(&text[range.clone()], 0.0, strong);
                    job.append(&text[range.end..], 0.0, plain);
                    let response = ui.add(egui::Button::new(job).frame(false));
                    response.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::Role::Button,
                            true,
                            format!("{}: {text}", hit.title),
                        )
                    });
                    if response.clicked() {
                        out.push(Command::SearchGo(index));
                    }
                });
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};

    #[derive(Default)]
    struct Model {
        state: State,
        query: Query,
        out: Vec<Command>,
    }

    fn harness<'a>(state: State) -> Harness<'a, Model> {
        Harness::builder()
            .with_size(egui::vec2(900.0, 800.0))
            .allow_missing_glyphs()
            .build_ui_state(
                |ui, m: &mut Model| {
                    draw(
                        ui,
                        &Scene {
                            state: &m.state,
                            writable: true,
                            open: Some("book/a"),
                            current: &|a| Some(a.quote != "stale"),
                            tr: &|k, _| k.to_owned(),
                        },
                        &mut m.query,
                        &mut m.out,
                    );
                },
                Model {
                    state,
                    ..Model::default()
                },
            )
    }

    fn annotation(quote: &str) -> Annotation {
        Annotation {
            id: "00000000-0000-4000-8000-000000000001".into(),
            kind: Kind::Highlight,
            book: "book/a".into(),
            title: "A".into(),
            chapter: 0,
            paragraph: 0,
            start: 0,
            end: quote.len() as u32,
            quote: quote.into(),
            name: String::new(),
            note: String::new(),
            created_ms: 1,
            changed_ms: 1,
        }
    }

    // T-RDR-002: a stale annotation cannot be opened; a current one can.
    #[test]
    fn stale_annotations_are_marked_and_not_opened() {
        let mut h = harness(State {
            items: vec![annotation("stale")],
            ..State::default()
        });
        h.run();
        h.get_by_label("annotations-stale");
        h.get_by_label("annotations-go").click();
        h.run();
        assert!(
            h.state()
                .out
                .iter()
                .all(|c| !matches!(c, Command::AnnotationGo(_)))
        );
        h.state_mut().state.items = vec![annotation("猫")];
        h.run();
        h.get_by_label("annotations-go").click();
        h.run();
        assert!(
            h.state()
                .out
                .iter()
                .any(|c| matches!(c, Command::AnnotationGo(_)))
        );
    }

    // T-RDR-001: the editor saves only on an explicit valid click and offers scopes.
    #[test]
    fn the_editor_needs_an_explicit_valid_save() {
        let mut draft = annotation("猫");
        draft.kind = Kind::Bookmark;
        let mut h = harness(State {
            editor: Some(draft),
            ..State::default()
        });
        h.run();
        use egui_kittest::kittest::NodeT;
        assert!(
            h.get_by_label("phrases-save")
                .accesskit_node()
                .is_disabled(),
            "a bookmark needs a name"
        );
        h.state_mut().state.editor.as_mut().unwrap().kind = Kind::Note;
        h.run();
        h.get_by_label("annotations-cover-sentence").click();
        h.run();
        h.get_by_label("phrases-save").click();
        h.run();
        let out = &h.state().out;
        assert!(
            out.iter()
                .any(|c| matches!(c, Command::AnnotationScope(Scope::Sentence)))
        );
        assert!(out.iter().any(|c| matches!(c, Command::AnnotationSave)));
    }

    // T-RDR-003: the search is explicit and its results open.
    #[test]
    fn search_results_open_by_click() {
        let mut state = State {
            tab: Tab::Search,
            ..State::default()
        };
        state.search.outcome = Some(crate::reader_tools::Outcome {
            epoch: 0,
            hits: vec![crate::reader_tools::Hit {
                uid: "book/a".into(),
                title: "A".into(),
                chapter: 0,
                paragraph: 1,
                start: 0,
                end: 3,
                text: "猫".into(),
                snippet: ("猫です".into(), 0..3),
            }],
            searched: 1,
            failed: vec!["B".into()],
            truncated: false,
        });
        let mut h = harness(state);
        h.run();
        assert!(h.query_by_label("search-run").is_some());
        h.get_by_label("A: 猫です").click();
        h.run();
        assert!(
            h.state()
                .out
                .iter()
                .any(|c| matches!(c, Command::SearchGo(0)))
        );
        h.get_by_label("search-unreadable");
    }
}
