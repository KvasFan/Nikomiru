// SPDX-License-Identifier: GPL-3.0-only

//! The app's own file chooser (APP-003), for systems without a file dialog
//! the app can reach (Android, D-16): a window listing a folder, with the
//! dialog's filters, its parent and its subfolders; a folder dialog takes the
//! folder shown, a save dialog a name typed in it.

use std::path::{Path, PathBuf};

use crate::i18n::Translator;

use super::FileDialog;

/// One entry of the folder shown.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    name: String,
    folder: bool,
}

/// A chooser open on a folder.
#[derive(Debug)]
pub struct FileChooser {
    /// The dialog it answers.
    pub dialog: FileDialog,
    folder: PathBuf,
    entries: Vec<Entry>,
    /// Why the folder could not be listed.
    error: Option<String>,
    /// Which of the dialog's filters applies.
    filter: usize,
    /// The name typed, in a save dialog.
    name: String,
}

/// What the user did with the chooser this frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
    /// Still open.
    Open,
    /// Closed without a choice.
    Cancelled,
    /// This file or folder.
    Chosen(PathBuf),
}

/// Whether `name` matches a glob pattern of a filter: `*` or `*.ext`,
/// without regard to case.
fn matches(name: &str, pattern: &str) -> bool {
    match pattern.strip_prefix("*.") {
        Some(extension) => name
            .rsplit_once('.')
            .is_some_and(|(_, e)| e.eq_ignore_ascii_case(extension)),
        None => pattern == "*",
    }
}

/// One row of the list: the whole width, the name from the left, large
/// enough for a finger.
fn row(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let size = egui::vec2(ui.available_width(), 40.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), text));
    if ui.is_rect_visible(rect) {
        let visuals = ui.style().interact(&response);
        if response.hovered() || response.has_focus() {
            ui.painter()
                .rect_filled(rect, visuals.corner_radius, visuals.weak_bg_fill);
        }
        ui.painter().text(
            rect.left_center() + egui::vec2(8.0, 0.0),
            egui::Align2::LEFT_CENTER,
            text,
            egui::TextStyle::Button.resolve(ui.style()),
            visuals.text_color(),
        );
    }
    response
}

impl FileChooser {
    /// A chooser for `dialog`, in the dialog's folder or else `start`.
    pub fn new(dialog: FileDialog, start: PathBuf) -> Self {
        let folder = dialog.folder.clone().unwrap_or(start);
        let name = dialog.save_as.clone().unwrap_or_default();
        let mut chooser = Self {
            dialog,
            folder,
            entries: Vec::new(),
            error: None,
            filter: 0,
            name,
        };
        chooser.list();
        chooser
    }

    /// Reads the folder shown again: folders first, then the files the
    /// filter lets through, each by name; hidden ones are left out.
    fn list(&mut self) {
        self.entries.clear();
        self.error = None;
        let patterns = self
            .dialog
            .filters
            .get(self.filter)
            .map(|(_, p)| p.clone())
            .unwrap_or_else(|| vec!["*"]);
        match std::fs::read_dir(&self.folder) {
            Ok(read) => {
                for entry in read.flatten() {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if name.starts_with('.') {
                        continue;
                    }
                    let folder = entry.path().is_dir();
                    if folder
                        || (!self.dialog.directory && patterns.iter().any(|p| matches(&name, p)))
                    {
                        self.entries.push(Entry { name, folder });
                    }
                }
                self.entries.sort_by(|a, b| {
                    b.folder
                        .cmp(&a.folder)
                        .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                });
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    fn go(&mut self, folder: PathBuf) {
        self.folder = folder;
        self.list();
    }

    /// Draws the chooser over the window.
    pub fn show(&mut self, ctx: &egui::Context, tr: &Translator) -> Choice {
        let mut choice = Choice::Open;
        let mut open = true;
        let mut go_to = None;
        let screen = ctx.content_rect();
        egui::Window::new(self.dialog.title.clone())
            .open(&mut open)
            // above the page it was opened from, which is a window too
            .order(egui::Order::Foreground)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .fixed_size(egui::vec2(
                (screen.width() - 32.0).clamp(200.0, 720.0),
                (screen.height() - 120.0).clamp(160.0, 900.0),
            ))
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    let parent = self.folder.parent().map(Path::to_path_buf);
                    if ui
                        .add_enabled(parent.is_some(), egui::Button::new("⬆"))
                        .on_hover_text(tr.tr("chooser-up", &[]))
                        .clicked()
                    {
                        go_to = parent;
                    }
                    if ui.button(tr.tr("chooser-refresh", &[])).clicked() {
                        go_to = Some(self.folder.clone());
                    }
                    ui.label(self.folder.display().to_string());
                });
                if self.dialog.filters.len() > 1 && !self.dialog.directory {
                    let before = self.filter;
                    egui::ComboBox::new("file-chooser-filter", tr.tr("chooser-filter", &[]))
                        .selected_text(
                            self.dialog
                                .filters
                                .get(self.filter)
                                .map(|(n, _)| n.clone())
                                .unwrap_or_default(),
                        )
                        .show_ui(ui, |ui| {
                            for (i, (name, _)) in self.dialog.filters.iter().enumerate() {
                                ui.selectable_value(&mut self.filter, i, name);
                            }
                        });
                    if self.filter != before {
                        go_to = Some(self.folder.clone());
                    }
                }
                ui.separator();
                let footer = 48.0;
                egui::ScrollArea::vertical()
                    .max_height((ui.available_height() - footer).max(60.0))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if let Some(error) = &self.error {
                            ui.label(tr.tr("chooser-unreadable", &[("error", error.clone())]));
                        } else if self.entries.is_empty() {
                            ui.label(tr.tr("chooser-empty", &[]));
                        }
                        for entry in &self.entries {
                            let text = if entry.folder {
                                format!("📁 {}", entry.name)
                            } else {
                                entry.name.clone()
                            };
                            let row = row(ui, &text);
                            if row.clicked() {
                                let path = self.folder.join(&entry.name);
                                if entry.folder {
                                    go_to = Some(path);
                                } else if self.dialog.save_as.is_some() {
                                    self.name = entry.name.clone();
                                } else {
                                    choice = Choice::Chosen(path);
                                }
                            }
                        }
                    });
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button(tr.tr("chooser-cancel", &[])).clicked() {
                        choice = Choice::Cancelled;
                    }
                    if self.dialog.directory {
                        if ui.button(tr.tr("chooser-use-folder", &[])).clicked() {
                            choice = Choice::Chosen(self.folder.clone());
                        }
                    } else if self.dialog.save_as.is_some() {
                        let name = self.name.trim().to_string();
                        let save = ui.add_enabled(
                            !name.is_empty() && !name.contains('/'),
                            egui::Button::new(tr.tr("chooser-save", &[])),
                        );
                        ui.add(egui::TextEdit::singleline(&mut self.name).desired_width(240.0));
                        if save.clicked() {
                            choice = Choice::Chosen(self.folder.join(name));
                        }
                    }
                });
            });
        if let Some(folder) = go_to {
            self.go(folder);
        }
        if !open {
            choice = Choice::Cancelled;
        }
        choice
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::Dialog;

    fn dialog(directory: bool) -> FileDialog {
        FileDialog {
            dialog: Dialog::Video,
            title: "Open".into(),
            filters: vec![
                ("Video".into(), vec!["*.mkv", "*.mp4"]),
                ("All".into(), vec!["*"]),
            ],
            save_as: None,
            folder: None,
            directory,
        }
    }

    #[test]
    fn patterns_match_by_extension_without_case() {
        assert!(matches("a.MKV", "*.mkv"));
        assert!(matches("a.b.mp4", "*.mp4"));
        assert!(!matches("mkv", "*.mkv"));
        assert!(!matches("a.mkv.part", "*.mkv"));
        assert!(matches("anything", "*"));
    }

    #[test]
    fn a_folder_lists_its_folders_first_then_the_files_of_the_filter() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("Season 1")).unwrap();
        std::fs::create_dir(dir.path().join(".hidden")).unwrap();
        for name in ["b.mkv", "A.mp4", "notes.txt"] {
            std::fs::write(dir.path().join(name), b"").unwrap();
        }
        let mut chooser = FileChooser::new(dialog(false), dir.path().to_path_buf());
        let names: Vec<_> = chooser.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["Season 1", "A.mp4", "b.mkv"]);
        // all files
        chooser.filter = 1;
        chooser.list();
        assert_eq!(chooser.entries.len(), 4);
        // a folder dialog shows folders only
        let chooser = FileChooser::new(dialog(true), dir.path().to_path_buf());
        let names: Vec<_> = chooser.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["Season 1"]);
    }

    #[test]
    fn the_chooser_shows_its_folder_and_takes_a_click() {
        use egui_kittest::Harness;
        use egui_kittest::kittest::Queryable;
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.mkv"), b"").unwrap();
        let tr = Translator::new("en");
        let mut chooser = FileChooser::new(dialog(false), dir.path().to_path_buf());
        let mut chosen = None;
        {
            let mut h = Harness::builder()
                .with_size(egui::vec2(412.0, 900.0))
                .allow_missing_glyphs()
                .build_ui(|ui| {
                    if let Choice::Chosen(p) = chooser.show(ui.ctx(), &tr) {
                        chosen = Some(p);
                    }
                });
            h.run();
            h.get_by_label("a.mkv").click();
            h.run();
        }
        assert_eq!(chosen, Some(dir.path().join("a.mkv")));
    }

    #[test]
    fn an_unreadable_folder_says_why() {
        let chooser = FileChooser::new(dialog(false), PathBuf::from("/no/such/folder"));
        assert!(chooser.error.is_some());
        assert!(chooser.entries.is_empty());
    }
}
