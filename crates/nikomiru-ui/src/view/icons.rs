// SPDX-License-Identifier: GPL-3.0-only

//! Line icons drawn with egui's painter (SET-010): the navigation rail, the
//! settings' tabs and the page headers carry them as Fushi 3's do. No icon
//! font is bundled (SPECIFICATION §3.4), so each icon is a few strokes on a
//! 24-unit grid, the grid of Material Symbols, scaled to the rect it gets.

use egui::epaint::{CubicBezierShape, PathShape, PathStroke};
use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, vec2};

/// The icons the interface draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Icon {
    Home,
    Video,
    Book,
    Manga,
    Web,
    Chart,
    Review,
    Server,
    Settings,
    Tools,
    Play,
    Subtitles,
    Cloud,
    Backup,
    Dictionary,
    Search,
    Audio,
    Card,
    Keyboard,
    Code,
    Info,
}

/// A point of the 24-unit grid inside `rect`.
struct Grid {
    origin: Pos2,
    unit: f32,
}

impl Grid {
    fn new(rect: Rect) -> Self {
        let side = rect.width().min(rect.height());
        let origin = rect.center() - vec2(side, side) / 2.0;
        Self {
            origin,
            unit: side / 24.0,
        }
    }

    fn p(&self, x: f32, y: f32) -> Pos2 {
        self.origin + vec2(x, y) * self.unit
    }

    fn r(&self, x0: f32, y0: f32, x1: f32, y1: f32) -> Rect {
        Rect::from_min_max(self.p(x0, y0), self.p(x1, y1))
    }

    fn len(&self, v: f32) -> f32 {
        v * self.unit
    }
}

/// Paints `icon` in `rect` with `color`, its strokes as thick as Material
/// Symbols' at weight 400 (2 units of 24).
pub(crate) fn paint(painter: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let g = Grid::new(rect);
    let stroke = Stroke::new(g.len(1.8).max(1.2), color);
    let line = |points: &[(f32, f32)]| {
        painter.add(Shape::line(
            points.iter().map(|(x, y)| g.p(*x, *y)).collect(),
            stroke,
        ));
    };
    let closed = |points: &[(f32, f32)]| {
        painter.add(Shape::Path(PathShape::closed_line(
            points.iter().map(|(x, y)| g.p(*x, *y)).collect(),
            stroke,
        )));
    };
    let filled = |points: &[(f32, f32)]| {
        painter.add(Shape::convex_polygon(
            points.iter().map(|(x, y)| g.p(*x, *y)).collect(),
            color,
            Stroke::NONE,
        ));
    };
    let frame = |x0: f32, y0: f32, x1: f32, y1: f32, radius: f32| {
        painter.rect_stroke(
            g.r(x0, y0, x1, y1),
            g.len(radius),
            stroke,
            egui::StrokeKind::Middle,
        );
    };
    let circle = |x: f32, y: f32, r: f32| {
        painter.circle_stroke(g.p(x, y), g.len(r), stroke);
    };
    let dot = |x: f32, y: f32, r: f32| {
        painter.circle_filled(g.p(x, y), g.len(r), color);
    };
    let curve = |a: (f32, f32), b: (f32, f32), c: (f32, f32), d: (f32, f32)| {
        painter.add(CubicBezierShape::from_points_stroke(
            [g.p(a.0, a.1), g.p(b.0, b.1), g.p(c.0, c.1), g.p(d.0, d.1)],
            false,
            Color32::TRANSPARENT,
            PathStroke::from(stroke),
        ));
    };
    match icon {
        Icon::Home => {
            closed(&[
                (4.0, 10.5),
                (12.0, 4.0),
                (20.0, 10.5),
                (20.0, 20.0),
                (4.0, 20.0),
            ]);
            line(&[(10.0, 20.0), (10.0, 14.0), (14.0, 14.0), (14.0, 20.0)]);
        }
        Icon::Video => {
            // a clapperboard
            frame(3.5, 9.0, 20.5, 20.0, 1.5);
            closed(&[(3.5, 9.0), (3.5, 5.5), (20.5, 4.0), (20.5, 7.5)]);
            line(&[(8.0, 5.1), (9.5, 8.4)]);
            line(&[(13.0, 4.6), (14.5, 7.9)]);
        }
        Icon::Book => {
            // an open book
            closed(&[(12.0, 7.0), (12.0, 19.5), (3.0, 18.0), (3.0, 5.5)]);
            closed(&[(12.0, 7.0), (21.0, 5.5), (21.0, 18.0), (12.0, 19.5)]);
        }
        Icon::Manga => {
            // a picture with a page behind it
            frame(7.0, 3.5, 20.5, 17.0, 1.5);
            line(&[(3.5, 7.0), (3.5, 20.5), (17.0, 20.5)]);
            line(&[
                (9.5, 14.5),
                (12.5, 10.5),
                (14.5, 13.0),
                (16.0, 11.5),
                (18.5, 14.5),
            ]);
        }
        Icon::Web => {
            circle(12.0, 12.0, 8.5);
            line(&[(3.5, 12.0), (20.5, 12.0)]);
            curve((12.0, 3.5), (8.0, 7.0), (8.0, 17.0), (12.0, 20.5));
            curve((12.0, 3.5), (16.0, 7.0), (16.0, 17.0), (12.0, 20.5));
        }
        Icon::Chart => {
            line(&[(6.0, 20.0), (6.0, 12.0)]);
            line(&[(12.0, 20.0), (12.0, 5.0)]);
            line(&[(18.0, 20.0), (18.0, 9.0)]);
        }
        Icon::Review => {
            // two arrows going round
            curve((5.0, 11.0), (5.5, 6.5), (11.0, 4.0), (16.5, 6.5));
            line(&[(13.5, 4.0), (17.5, 6.8), (14.5, 10.0)]);
            curve((19.0, 13.0), (18.5, 17.5), (13.0, 20.0), (7.5, 17.5));
            line(&[(10.5, 20.0), (6.5, 17.2), (9.5, 14.0)]);
        }
        Icon::Server => {
            frame(4.0, 4.0, 20.0, 10.5, 1.5);
            frame(4.0, 13.5, 20.0, 20.0, 1.5);
            dot(7.5, 7.25, 1.1);
            dot(7.5, 16.75, 1.1);
        }
        Icon::Settings => {
            // sliders ("tune")
            line(&[(4.0, 7.0), (13.0, 7.0)]);
            line(&[(17.0, 7.0), (20.0, 7.0)]);
            line(&[(15.0, 4.5), (15.0, 9.5)]);
            line(&[(4.0, 17.0), (7.0, 17.0)]);
            line(&[(11.0, 17.0), (20.0, 17.0)]);
            line(&[(9.0, 14.5), (9.0, 19.5)]);
        }
        Icon::Tools => {
            // four tiles
            frame(4.0, 4.0, 10.5, 10.5, 1.5);
            frame(13.5, 4.0, 20.0, 10.5, 1.5);
            frame(4.0, 13.5, 10.5, 20.0, 1.5);
            frame(13.5, 13.5, 20.0, 20.0, 1.5);
        }
        Icon::Play => {
            filled(&[(8.0, 5.0), (19.0, 12.0), (8.0, 19.0)]);
        }
        Icon::Subtitles => {
            frame(3.0, 5.0, 21.0, 19.0, 2.0);
            line(&[(6.5, 12.5), (10.0, 12.5)]);
            line(&[(12.5, 12.5), (17.5, 12.5)]);
            line(&[(6.5, 15.5), (13.0, 15.5)]);
            line(&[(15.5, 15.5), (17.5, 15.5)]);
        }
        Icon::Cloud => {
            curve((7.0, 18.0), (2.5, 18.0), (2.5, 11.5), (7.0, 11.5));
            curve((7.0, 11.5), (7.5, 5.0), (16.5, 5.0), (17.0, 10.0));
            curve((17.0, 10.0), (22.0, 10.0), (22.0, 18.0), (17.0, 18.0));
            line(&[(7.0, 18.0), (17.0, 18.0)]);
        }
        Icon::Backup => {
            // a clock with an arrow going back
            curve((4.5, 12.0), (4.5, 2.0), (19.5, 2.0), (19.5, 12.0));
            curve((19.5, 12.0), (19.5, 22.0), (4.5, 22.0), (4.5, 12.0));
            line(&[(12.0, 7.5), (12.0, 12.0), (15.0, 14.0)]);
            line(&[(2.0, 9.5), (4.5, 12.5), (7.0, 9.5)]);
        }
        Icon::Dictionary => {
            frame(5.0, 3.5, 19.0, 20.5, 1.5);
            line(&[(9.0, 3.5), (9.0, 20.5)]);
            line(&[(12.0, 8.5), (16.0, 8.5)]);
            line(&[(12.0, 11.5), (16.0, 11.5)]);
        }
        Icon::Search => {
            circle(10.0, 10.0, 6.0);
            line(&[(14.5, 14.5), (20.0, 20.0)]);
        }
        Icon::Audio => {
            closed(&[
                (4.0, 9.5),
                (8.0, 9.5),
                (12.5, 5.5),
                (12.5, 18.5),
                (8.0, 14.5),
                (4.0, 14.5),
            ]);
            curve((15.5, 9.0), (17.0, 10.5), (17.0, 13.5), (15.5, 15.0));
            curve((18.0, 6.5), (21.0, 9.5), (21.0, 14.5), (18.0, 17.5));
        }
        Icon::Card => {
            // two cards, one over the other
            frame(7.0, 7.0, 20.5, 20.0, 2.0);
            line(&[(3.5, 16.0), (3.5, 5.5), (5.5, 3.5), (16.0, 3.5)]);
        }
        Icon::Keyboard => {
            frame(2.5, 6.0, 21.5, 18.0, 2.0);
            for x in [6.0, 9.5, 13.0, 16.5] {
                dot(x, 10.0, 0.9);
            }
            line(&[(7.5, 14.5), (16.5, 14.5)]);
        }
        Icon::Code => {
            line(&[(8.5, 7.0), (3.5, 12.0), (8.5, 17.0)]);
            line(&[(15.5, 7.0), (20.5, 12.0), (15.5, 17.0)]);
            line(&[(13.5, 5.0), (10.5, 19.0)]);
        }
        Icon::Info => {
            circle(12.0, 12.0, 8.5);
            line(&[(12.0, 11.0), (12.0, 16.5)]);
            dot(12.0, 7.8, 1.1);
        }
    }
}

/// An icon button: `icon` on a round tonal fill that shows under the
/// pointer, named `name` for screen readers and in its tooltip; whether it
/// was clicked.
pub(crate) fn button(ui: &mut egui::Ui, icon: Icon, name: &str) -> egui::Response {
    let size = vec2(36.0, 36.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let visuals = ui.style().interact(&response);
    if response.hovered() || response.has_focus() || response.is_pointer_button_down_on() {
        ui.painter()
            .circle_filled(rect.center(), rect.width() / 2.0, visuals.weak_bg_fill);
    }
    if response.has_focus() {
        ui.painter().circle_stroke(
            rect.center(),
            rect.width() / 2.0,
            Stroke::new(2.0, super::palette::current().accent_text),
        );
    }
    let color = if ui.is_enabled() {
        visuals.text_color()
    } else {
        ui.visuals().weak_text_color()
    };
    paint(
        ui.painter(),
        Rect::from_center_size(rect.center(), vec2(20.0, 20.0)),
        icon,
        color,
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), name));
    response.on_hover_text(name)
}

/// An item of a navigation rail or of a column of tabs (SET-010): the icon
/// and the name, on the accent's tonal pill when chosen; its response, named
/// `text` and selected when `chosen` for screen readers.
pub(crate) fn nav_item(ui: &mut egui::Ui, icon: Icon, text: &str, chosen: bool) -> bool {
    nav_item_sized(ui, icon, text, chosen, 48.0)
}

/// [`nav_item`] `height` high.
pub(crate) fn nav_item_sized(
    ui: &mut egui::Ui,
    icon: Icon,
    text: &str,
    chosen: bool,
    height: f32,
) -> bool {
    use super::{palette, theme};
    let p = palette::current();
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), egui::Sense::click());
    let fill = if chosen {
        Some(p.selection)
    } else if response.hovered() || response.has_focus() {
        Some(p.row)
    } else {
        None
    };
    if let Some(fill) = fill {
        ui.painter().rect_filled(rect, palette::PILL, fill);
    }
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect,
            palette::PILL,
            Stroke::new(2.0, p.accent_text),
            egui::StrokeKind::Inside,
        );
    }
    let ink = if chosen { p.text } else { p.details };
    let side = (height * 0.45).clamp(16.0, 22.0);
    paint(
        ui.painter(),
        Rect::from_center_size(
            egui::pos2(rect.left() + 28.0, rect.center().y),
            vec2(side, side),
        ),
        icon,
        ink,
    );
    let family = if chosen {
        theme::bold()
    } else {
        egui::FontFamily::Proportional
    };
    let galley = ui.painter().layout(
        text.to_string(),
        egui::FontId::new(15.0, family),
        ink,
        (rect.width() - 64.0).max(0.0),
    );
    let at = egui::pos2(rect.left() + 52.0, rect.center().y - galley.size().y / 2.0);
    ui.painter().galley(at, galley, ink);
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, chosen, text));
    response.clicked()
}

/// An item of a compact navigation rail (SET-010): the icon on a pill, on
/// the accent's tonal fill when chosen, the name small under it; whether it
/// was clicked.
pub(crate) fn rail_item(ui: &mut egui::Ui, icon: Icon, text: &str, chosen: bool) -> bool {
    use super::{palette, theme};
    let p = palette::current();
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(vec2(width, 60.0), egui::Sense::click());
    let pill = Rect::from_center_size(
        egui::pos2(rect.center().x, rect.top() + 20.0),
        vec2(56.0, 32.0),
    );
    let fill = if chosen {
        Some(p.selection)
    } else if response.hovered() || response.has_focus() {
        Some(p.row)
    } else {
        None
    };
    if let Some(fill) = fill {
        ui.painter().rect_filled(pill, palette::PILL, fill);
    }
    if response.has_focus() {
        ui.painter().rect_stroke(
            pill,
            palette::PILL,
            Stroke::new(2.0, p.accent_text),
            egui::StrokeKind::Inside,
        );
    }
    let ink = if chosen { p.text } else { p.details };
    paint(
        ui.painter(),
        Rect::from_center_size(pill.center(), vec2(20.0, 20.0)),
        icon,
        ink,
    );
    let family = if chosen {
        theme::bold()
    } else {
        egui::FontFamily::Proportional
    };
    let mut job = egui::text::LayoutJob::simple_singleline(
        text.to_string(),
        egui::FontId::new(12.0, family),
        ink,
    );
    job.wrap = egui::text::TextWrapping::truncate_at_width(rect.width());
    let galley = ui.painter().layout_job(job);
    let at = egui::pos2(rect.center().x - galley.size().x / 2.0, pill.bottom() + 3.0);
    ui.painter().galley(at, galley, ink);
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, chosen, text));
    response.on_hover_text(text).clicked()
}

/// The center of a 24-unit grid point, for tests.
#[cfg(test)]
fn grid_point(rect: Rect, x: f32, y: f32) -> Pos2 {
    Grid::new(rect).p(x, y)
}

#[cfg(test)]
mod tests {
    use egui::pos2;

    use super::*;

    #[test]
    fn the_grid_is_centered_and_square() {
        let rect = Rect::from_min_size(pos2(10.0, 20.0), vec2(48.0, 24.0));
        // the square is as high as the rect, in its middle
        assert_eq!(grid_point(rect, 0.0, 0.0), pos2(22.0, 20.0));
        assert_eq!(grid_point(rect, 24.0, 24.0), pos2(46.0, 44.0));
    }
}
