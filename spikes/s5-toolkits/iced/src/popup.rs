// SPDX-License-Identifier: GPL-3.0-only

//! Nikomiru's dictionary popup in iced, from the same view model as the Qt
//! popup. iced is retained in a functional way: `view` builds the widget tree
//! again after every message, then the tree's state (text layouts, scroll
//! offsets) is kept across rebuilds.

use std::sync::Arc;

use iced::font::Weight;
use iced::widget::text::Span;
use iced::widget::{column, container, rich_text, row, scrollable, shader, space, span, text};
use iced::{Alignment, Background, Border, Color, Element, Font, Length, Padding};
use s5_common::popup::{
    Block, Cell, Content, Entry, Piece, Run, Section, Style, Tag, chip_color, color, marker,
    styled_text,
};
use s5_common::probe::FrameProbe;

use crate::Message;
use crate::video::ProbeProgram;

const BASE: f32 = 15.0;
const TEXT: Color = Color::from_rgb8(0xe8, 0xe8, 0xe8);
const LINK: Color = Color::from_rgb8(0x8a, 0xb4, 0xff);
const BOLD: Font = Font {
    weight: Weight::Bold,
    ..Font::DEFAULT
};

fn rgba(c: [u8; 4]) -> Color {
    Color::from_rgba8(c[0], c[1], c[2], c[3] as f32 / 255.0)
}

fn style_color(c: &Option<String>) -> Option<Color> {
    c.as_deref().and_then(color).map(rgba)
}

/// The popup, `width` wide and at most `max_height` tall; `probe` is marked
/// when the frame with this generation is rendered.
pub fn view<'a>(
    content: &'a Content,
    width: f32,
    max_height: f32,
    probe: &Arc<FrameProbe>,
    generation: u64,
) -> Element<'a, Message> {
    let entries = column(content.entries.iter().map(entry)).spacing(14);
    let probe = shader(ProbeProgram {
        probe: Arc::clone(probe),
        generation,
    })
    .width(1)
    .height(1);
    container(column![probe, scrollable(entries).height(Length::Shrink)].spacing(0))
        .padding(10)
        .width(width)
        .max_height(max_height)
        .style(|_| container::Style {
            background: Some(Background::Color(Color::from_rgba8(
                0x1b,
                0x1d,
                0x22,
                0xf2 as f32 / 255.0,
            ))),
            border: Border {
                color: Color::from_rgba8(255, 255, 255, 0.25),
                width: 1.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        })
        .into()
}

fn chip<'a>(label: &'a str, fill: Color, size: f32) -> Element<'a, Message> {
    container(text(label).size(size).color(TEXT))
        .padding([2, 5])
        .style(move |_| container::Style {
            background: Some(Background::Color(fill)),
            border: Border {
                radius: 4.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
}

fn tags<'a>(tags: &'a [Tag], size: f32) -> Element<'a, Message> {
    row(tags
        .iter()
        .map(|t| chip(&t.name, rgba(chip_color(&t.category)), size)))
    .spacing(4)
    .wrap()
    .into()
}

/// Base text with its reading centred above it.
fn ruby<'a>(
    t: &'a str,
    rt: &'a str,
    size: f32,
    rt_size: f32,
    c: Color,
    rt_c: Color,
    font: Font,
) -> Element<'a, Message> {
    column![
        text(rt).size(rt_size).color(rt_c),
        text(t).size(size).color(c).font(font)
    ]
    .align_x(Alignment::Center)
    .into()
}

fn entry(e: &Entry) -> Element<'_, Message> {
    let mut parts: Vec<Element<'_, Message>> = Vec::new();
    parts.push(
        row(e.furigana.iter().map(|f| {
            ruby(
                &f.t,
                if f.r.is_empty() { " " } else { &f.r },
                26.0,
                12.0,
                Color::WHITE,
                Color::from_rgb8(0xc5, 0xcf, 0xf5),
                Font::DEFAULT,
            )
        }))
        .align_y(Alignment::End)
        .into(),
    );
    if !e.trace.is_empty() || !e.term_tags.is_empty() {
        let trace = e
            .trace
            .iter()
            .map(|t| chip(&t.name, Color::from_rgb8(0x24, 0x46, 0x5a), 12.0));
        let term = e
            .term_tags
            .iter()
            .map(|t| chip(&t.name, rgba(chip_color(&t.category)), 12.0));
        parts.push(row(trace.chain(term)).spacing(4).wrap().into());
    }
    if !e.frequencies.is_empty() {
        parts.push(
            row(e.frequencies.iter().map(|f| {
                let values: Vec<String> = f.values.iter().map(|v| v.to_string()).collect();
                container(
                    text(format!("{}: {}", f.dictionary, values.join(", ")))
                        .size(12)
                        .color(TEXT),
                )
                .padding([2, 5])
                .style(|_| container::Style {
                    background: Some(Background::Color(Color::from_rgb8(0x2f, 0x3a, 0x2a))),
                    border: Border {
                        radius: 4.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .into()
            }))
            .spacing(4)
            .wrap()
            .into(),
        );
    }
    for s in &e.blocks {
        parts.push(section(s));
    }
    column(parts).spacing(6).into()
}

fn section(s: &Section) -> Element<'_, Message> {
    let arrow = if s.expanded { "▾" } else { "▸" };
    let title = container(
        text(format!("{arrow} {}", s.dictionary))
            .size(13)
            .font(BOLD)
            .color(Color::from_rgb8(0xc9, 0xd1, 0xdd)),
    )
    .width(Length::Fill)
    .padding([3, 6])
    .style(|_| container::Style {
        background: Some(Background::Color(Color::from_rgb8(0x26, 0x2a, 0x31))),
        border: Border {
            radius: 4.0.into(),
            ..Default::default()
        },
        ..Default::default()
    });
    let mut parts: Vec<Element<'_, Message>> = vec![title.into()];
    if s.expanded {
        for g in &s.glossaries {
            if !g.tags.is_empty() {
                parts.push(tags(&g.tags, 11.0));
            }
            parts.push(blocks(&g.blocks));
        }
    }
    column(parts).spacing(4).into()
}

fn blocks(blocks_: &[Block]) -> Element<'_, Message> {
    column(blocks_.iter().map(block))
        .spacing(3)
        .width(Length::Fill)
        .into()
}

fn block(b: &Block) -> Element<'_, Message> {
    match b {
        Block::Text { html, .. } => paragraph(html),
        Block::Flow { runs } => flow(runs),
        Block::Box { s, blocks: inner } => boxed(s, inner),
        Block::List {
            ordered,
            s,
            items,
            markers,
        } => {
            let left = s.margin.px(BASE)[3];
            column(items.iter().enumerate().map(|(i, item)| {
                let m = marker(
                    markers.get(i).and_then(|m| m.as_deref()),
                    s.list_marker.as_deref(),
                    *ordered,
                    i,
                );
                let bullet: Element<'_, Message> = if m.is_empty() {
                    space().width(0).into()
                } else {
                    text(m)
                        .size(BASE)
                        .color(Color::from_rgb8(0xaa, 0xb2, 0xbf))
                        .width(BASE.max(BASE * 1.2))
                        .into()
                };
                row![space().width(left), bullet, blocks(item)]
                    .spacing(4)
                    .into()
            }))
            .spacing(2)
            .into()
        }
        Block::Table { rows, .. } => table(rows),
        Block::Details {
            open,
            summary,
            blocks: inner,
        } => {
            let title: String = summary
                .iter()
                .filter_map(|r| match r {
                    Run::Text { t, .. } | Run::Link { t, .. } | Run::Ruby { t, .. } => {
                        Some(t.as_str())
                    }
                    _ => None,
                })
                .collect();
            let arrow = if *open { "▾ " } else { "▸ " };
            let mut parts: Vec<Element<'_, Message>> = vec![
                text(format!("{arrow}{title}"))
                    .size(BASE)
                    .color(TEXT)
                    .into(),
            ];
            if *open {
                parts.push(
                    container(blocks(inner))
                        .padding(Padding::default().left(16))
                        .into(),
                );
            }
            column(parts).spacing(2).into()
        }
        Block::Pairs { items } => column(items.iter().map(|(term, item_tags)| {
            let mut parts: Vec<Element<'_, Message>> =
                vec![text(term).size(BASE).color(TEXT).into()];
            parts.extend(
                item_tags
                    .iter()
                    .map(|t| chip(t, rgba(chip_color("")), 12.0)),
            );
            row(parts).spacing(4).wrap().into()
        }))
        .spacing(2)
        .into(),
    }
}

/// A StyledText paragraph: one rich text.
fn paragraph(html: &str) -> Element<'_, Message> {
    let spans: Vec<Span<'static, (), Font>> = styled_text(html)
        .into_iter()
        .map(|piece| match piece {
            Piece::Break => span("\n").size(BASE),
            Piece::Span {
                text: t,
                color: c,
                scale,
                bold,
                italic,
                underline,
                strike,
                link,
            } => {
                let colour = if link.is_some() {
                    LINK
                } else {
                    c.map_or(TEXT, rgba)
                };
                let font = Font {
                    weight: if bold { Weight::Bold } else { Weight::Normal },
                    style: if italic {
                        iced::font::Style::Italic
                    } else {
                        iced::font::Style::Normal
                    },
                    ..Font::DEFAULT
                };
                span(t)
                    .size(BASE * scale)
                    .color(colour)
                    .font(font)
                    .underline(underline || link.is_some())
                    .strikethrough(strike)
            }
        })
        .collect();
    rich_text(spans).width(Length::Fill).into()
}

fn run_text<'a>(t: &'a str, s: &Style) -> iced::widget::Text<'a> {
    let font = Font {
        weight: if s.bold == Some(true) {
            Weight::Bold
        } else {
            Weight::Normal
        },
        style: if s.italic == Some(true) {
            iced::font::Style::Italic
        } else {
            iced::font::Style::Normal
        },
        ..Font::DEFAULT
    };
    text(t)
        .size(BASE * s.font_scale.unwrap_or(1.0))
        .color(style_color(&s.color).unwrap_or(TEXT))
        .font(font)
}

/// Inline runs: characters and words wrap; ruby stands over its base; the
/// row is aligned at the bottom, so text sits on the base line of the ruby.
fn flow(runs: &[Run]) -> Element<'_, Message> {
    let mut items: Vec<Element<'_, Message>> = Vec::with_capacity(runs.len());
    for r in runs {
        items.push(match r {
            Run::Text { t, s } if s.background.is_some() || s.border_color.is_some() => {
                let [pt, pr, pb, pl] = s.padding.px(BASE);
                let [_, mr, _, ml] = s.margin.px(BASE);
                let fill = style_color(&s.background).unwrap_or(Color::TRANSPARENT);
                let radius = s.border_radius.map_or(0.0, |l| l.px(BASE));
                let chip = container(run_text(t, s))
                    .padding(Padding {
                        top: pt,
                        right: pr,
                        bottom: pb,
                        left: pl,
                    })
                    .style(move |_| container::Style {
                        background: Some(Background::Color(fill)),
                        border: Border {
                            radius: radius.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    });
                row![space().width(ml), chip, space().width(mr)].into()
            }
            Run::Text { t, s } => run_text(t, s).into(),
            Run::Ruby { t, rt, s } => {
                let c = style_color(&s.color).unwrap_or(TEXT);
                let font = if s.bold == Some(true) {
                    BOLD
                } else {
                    Font::DEFAULT
                };
                ruby(
                    t,
                    rt,
                    BASE * s.font_scale.unwrap_or(1.0),
                    BASE * 0.55,
                    c,
                    c,
                    font,
                )
            }
            Run::Link { t, s, .. } => run_text(t, s).color(LINK).into(),
            Run::Image { .. } => text("[image]")
                .size(BASE * 0.85)
                .color(Color::from_rgb8(0x9a, 0xa0, 0xaa))
                .into(),
            Run::Br => space().width(Length::Fill).into(),
        });
    }
    row(items).align_y(Alignment::End).wrap().into()
}

fn boxed<'a>(s: &Style, inner: &'a [Block]) -> Element<'a, Message> {
    let [mt, mr, mb, ml] = s.margin.px(BASE);
    let [pt, pr, pb, pl] = s.padding.px(BASE);
    let left_only = s
        .border_sides
        .is_some_and(|b| b[3] && !(b[0] && b[1] && b[2]));
    let line = s
        .border_width
        .map_or(if left_only { 3.0 } else { 1.0 }, |w| w.px(BASE).max(1.0));
    let border = style_color(&s.border_color);
    let fill = style_color(&s.background).unwrap_or(Color::TRANSPARENT);
    let radius = if left_only {
        0.0
    } else {
        s.border_radius.map_or(0.0, |l| l.px(BASE))
    };
    let content = container(blocks(inner))
        .padding(Padding {
            top: pt,
            right: pr,
            bottom: pb,
            left: pl,
        })
        .width(Length::Fill);
    let framed: Element<'a, Message> = if left_only {
        let bar_color = border.unwrap_or(Color::from_rgba8(0x90, 0x9a, 0xa8, 0.5));
        let bar = container(space())
            .width(line)
            .height(Length::Fill)
            .style(move |_| container::Style {
                background: Some(Background::Color(bar_color)),
                ..Default::default()
            });
        container(row![bar, content].spacing(4).height(Length::Shrink))
            .style(move |_| container::Style {
                background: Some(Background::Color(fill)),
                ..Default::default()
            })
            .into()
    } else {
        content
            .style(move |_| container::Style {
                background: Some(Background::Color(fill)),
                border: Border {
                    color: border.unwrap_or(Color::TRANSPARENT),
                    width: if border.is_some() { line } else { 0.0 },
                    radius: radius.into(),
                },
                ..Default::default()
            })
            .into()
    };
    container(framed)
        .padding(Padding {
            top: mt,
            right: mr,
            bottom: mb,
            left: ml,
        })
        .into()
}

/// A table: one row of cells per table row (spans are not laid out).
fn table(rows: &[Vec<Cell>]) -> Element<'_, Message> {
    let grid = column(rows.iter().map(|r| {
        row(r.iter().map(|cell| {
            let fill = style_color(&cell.s.background).unwrap_or(if cell.header {
                Color::from_rgb8(0x2c, 0x30, 0x38)
            } else {
                Color::TRANSPARENT
            });
            container(blocks(&cell.blocks))
                .padding([4, 6])
                .width((cell.natural_em * BASE + 14.0).clamp(32.0, 280.0))
                .style(move |_| container::Style {
                    background: Some(Background::Color(fill)),
                    border: Border {
                        color: Color::from_rgb8(0x55, 0x5b, 0x66),
                        width: 1.0,
                        radius: 0.0.into(),
                    },
                    ..Default::default()
                })
                .into()
        }))
        .into()
    }));
    scrollable(grid)
        .direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::default(),
        ))
        .into()
}
