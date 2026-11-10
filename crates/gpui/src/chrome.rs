//! Reader chrome that floats over the page: the shortcut dock, the
//! right-click action wheel, and the stack of chapter notes.
//!
//! Each piece stays quiet until the pointer reaches it — dimmed, compact, or
//! collapsed — so the reading column remains the only loud thing on screen.

use gpui_kit::assets::IconName;
use gpui_kit::component::{ActiveTheme, Icon, h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;
use oma_bible_core::storage::DockPosition;

use crate::app::OmaBibleApp;
use crate::tools::Tool;

const WHEEL_SIZE: f32 = 236.;
const WHEEL_RADIUS: f32 = 84.;
const WHEEL_BUTTON: f32 = 46.;
const WHEEL_HUB: f32 = 100.;
const EDGE_GAP: f32 = 14.;

// ── Dock ────────────────────────────────────────────────────────────────

/// A small floating pill of tool icons, pinned to one edge. The pill is
/// opaque so text never shows through it; its icons stay muted until
/// hovered, and it is not rendered at all when hidden.
pub fn render_dock(app: &OmaBibleApp, cx: &mut Context<OmaBibleApp>) -> Option<AnyElement> {
    let position = app.settings.dock;
    if position == DockPosition::Hidden {
        return None;
    }
    let vertical = matches!(position, DockPosition::Left | DockPosition::Right);
    let border = cx.theme().border;

    let mut items: Vec<AnyElement> = Vec::with_capacity(Tool::DOCK.len() + 1);
    let mut divided = false;
    for tool in Tool::DOCK {
        if !divided && !tool.is_verse_action() {
            divided = true;
            let rule = div().bg(border);
            items.push(
                if vertical {
                    rule.w(px(16.)).h(px(1.)).my(px(3.))
                } else {
                    rule.w(px(1.)).h(px(16.)).mx(px(3.))
                }
                .into_any_element(),
            );
        }
        items.push(dock_button(app, tool, cx));
    }

    let pill = if vertical { v_flex() } else { h_flex() }
        .id("shortcut-dock")
        // Clicks on the dock must not reach the verse underneath; scrolling
        // still moves the page.
        .block_mouse_except_scroll()
        .items_center()
        .gap(px(2.))
        .p(px(4.))
        .rounded_full()
        .bg(cx.theme().popover)
        .border_1()
        .border_color(border)
        .shadow_md()
        .children(items);

    let anchor = div().absolute().flex();
    let anchor = match position {
        DockPosition::Bottom => anchor
            .bottom(px(EDGE_GAP))
            .left_0()
            .right_0()
            .justify_center(),
        DockPosition::Top => anchor.top(px(EDGE_GAP)).left_0().right_0().justify_center(),
        DockPosition::Left => anchor.left(px(EDGE_GAP)).top_0().bottom_0().items_center(),
        DockPosition::Right | DockPosition::Hidden => {
            anchor.right(px(EDGE_GAP)).top_0().bottom_0().items_center()
        }
    };
    Some(anchor.child(pill).into_any_element())
}

fn dock_button(app: &OmaBibleApp, tool: Tool, cx: &mut Context<OmaBibleApp>) -> AnyElement {
    let active = app.tool_active(tool);
    let hover_bg = cx.theme().selection;
    let hover_fg = cx.theme().foreground;
    div()
        .id(SharedString::from(format!("dock-{}", tool.label())))
        .size(px(32.))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .cursor_pointer()
        .text_color(if active {
            cx.theme().accent
        } else {
            cx.theme().muted_foreground
        })
        .hover(move |style| style.bg(hover_bg).text_color(hover_fg))
        .on_click(cx.listener(move |this, _, window, cx| this.run_tool(tool, window, cx)))
        .child(Icon::new(tool.icon()).size(px(16.)))
        .into_any_element()
}

// ── Wheel ───────────────────────────────────────────────────────────────

/// Keep a wheel opened near a window edge fully on screen.
pub fn clamp_wheel_center(position: Point<Pixels>, viewport: Size<Pixels>) -> Point<Pixels> {
    let half = px(WHEEL_SIZE / 2. + 8.);
    let clamp = |value: Pixels, extent: Pixels| {
        if extent <= half * 2. {
            extent / 2.
        } else {
            value.clamp(half, extent - half)
        }
    };
    point(
        clamp(position.x, viewport.width),
        clamp(position.y, viewport.height),
    )
}

/// A radial menu: tools on a ring, a hub in the middle that names whatever
/// the pointer is over. Opens at the pointer on right-click, or centred with
/// Space; number keys and the tools' own shortcuts work while it is open.
pub fn render_wheel(app: &OmaBibleApp, cx: &mut Context<OmaBibleApp>) -> AnyElement {
    let (title, hint) = match app.wheel_hover {
        Some(tool) => (tool.label().to_string(), format!("Key  {}", tool.key())),
        None => (
            app.current_reference()
                .map(|(_, book, chapter, verse)| format!("{book} {chapter}:{verse}"))
                .unwrap_or_else(|| "No verse".to_string()),
            "Esc to close".to_string(),
        ),
    };

    let center = WHEEL_SIZE / 2.;
    let count = Tool::WHEEL.len() as f32;
    let buttons = Tool::WHEEL
        .iter()
        .enumerate()
        .map(|(index, &tool)| {
            let angle = std::f32::consts::TAU * index as f32 / count - std::f32::consts::FRAC_PI_2;
            let left = center + WHEEL_RADIUS * angle.cos() - WHEEL_BUTTON / 2.;
            let top = center + WHEEL_RADIUS * angle.sin() - WHEEL_BUTTON / 2.;
            wheel_button(app, tool, left, top, cx)
        })
        .collect::<Vec<_>>();

    let hub = v_flex()
        .absolute()
        .left(px(center - WHEEL_HUB / 2.))
        .top(px(center - WHEEL_HUB / 2.))
        .size(px(WHEEL_HUB))
        .rounded_full()
        .items_center()
        .justify_center()
        .gap(px(2.))
        .px(px(8.))
        .bg(cx.theme().muted)
        .child(
            div()
                .text_size(px(13.))
                .text_center()
                .line_clamp(2)
                .text_color(cx.theme().foreground)
                .child(title),
        )
        .child(
            div()
                .text_size(px(10.))
                .text_color(cx.theme().muted_foreground)
                .child(hint),
        );

    let wheel = div()
        .id("action-wheel")
        .relative()
        .flex_shrink_0()
        .size(px(WHEEL_SIZE))
        .rounded_full()
        .bg(cx.theme().popover)
        .border_1()
        .border_color(cx.theme().border)
        .shadow_xl()
        .on_click(|_, _, cx| cx.stop_propagation())
        .child(
            div()
                .absolute()
                .left(px(center - WHEEL_RADIUS))
                .top(px(center - WHEEL_RADIUS))
                .size(px(WHEEL_RADIUS * 2.))
                .rounded_full()
                .border_1()
                .border_color(cx.theme().border.opacity(0.5)),
        )
        .child(hub)
        .children(buttons);

    let mut scrim = div()
        .id("overlay-scrim")
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .bg(rgba(0x00000059))
        .occlude()
        .on_click(cx.listener(|this, _, _, cx| this.close_overlay(cx)))
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(|this, event: &MouseDownEvent, window, cx| {
                this.wheel_center =
                    Some(clamp_wheel_center(event.position, window.viewport_size()));
                this.wheel_hover = None;
                cx.stop_propagation();
                cx.notify();
            }),
        );

    scrim = match app.wheel_center {
        Some(at) => scrim.child(
            div()
                .absolute()
                .left(at.x - px(center))
                .top(at.y - px(center))
                .child(wheel),
        ),
        None => scrim.flex().items_center().justify_center().child(wheel),
    };
    scrim.into_any_element()
}

fn wheel_button(
    app: &OmaBibleApp,
    tool: Tool,
    left: f32,
    top: f32,
    cx: &mut Context<OmaBibleApp>,
) -> AnyElement {
    let hovered = app.wheel_hover == Some(tool);
    let active = app.tool_active(tool);
    let (bg, fg) = if hovered {
        (cx.theme().accent, cx.theme().accent_foreground)
    } else if active {
        (cx.theme().selection, cx.theme().accent)
    } else {
        (transparent_black(), cx.theme().muted_foreground)
    };
    div()
        .id(SharedString::from(format!("wheel-{}", tool.label())))
        .absolute()
        .left(px(left))
        .top(px(top))
        .size(px(WHEEL_BUTTON))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .cursor_pointer()
        .bg(bg)
        .text_color(fg)
        .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
            let next = if *hovered {
                Some(tool)
            } else if this.wheel_hover == Some(tool) {
                None
            } else {
                this.wheel_hover
            };
            if next != this.wheel_hover {
                this.wheel_hover = next;
                cx.notify();
            }
        }))
        .on_click(cx.listener(move |this, _, window, cx| this.run_tool(tool, window, cx)))
        .child(Icon::new(tool.icon()).size(px(19.)))
        .into_any_element()
}

// ── Notes stack ─────────────────────────────────────────────────────────

const PILE_WIDTH: f32 = 196.;
const PILE_CARD: f32 = 88.;
const PILE_HEIGHT: f32 = 132.;
const PILE_BEHIND: usize = 3;
const LIST_WIDTH: f32 = 300.;

/// The open chapter's notes at the middle of the right edge (the left edge
/// when the dock is there). At rest they lie in a loose pile, the back cards
/// tossed at random angles like a stack of screenshots; hovering spreads
/// them into a column of separate cards. Clicking a card opens its notes.
pub fn render_notes_stack(app: &OmaBibleApp, cx: &mut Context<OmaBibleApp>) -> Option<AnyElement> {
    let notes = app.chapter_notes();
    if notes.is_empty() {
        return None;
    }
    let body = if app.notes_expanded {
        notes_list(app, &notes, cx)
    } else {
        notes_pile(app, &notes, cx)
    };

    let stack = div()
        .id("notes-stack")
        // Clicks here must not select the verse underneath.
        .occlude()
        .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
            if this.notes_expanded != *hovered {
                this.notes_expanded = *hovered;
                cx.notify();
            }
        }))
        .child(body);

    let anchor = div().absolute().top_0().bottom_0().flex().items_center();
    let anchor = if app.settings.dock == DockPosition::Right {
        anchor.left(px(EDGE_GAP + 4.))
    } else {
        anchor.right(px(EDGE_GAP + 4.))
    };
    Some(anchor.child(stack).into_any_element())
}

/// First line of a note for a preview, without quote markers.
fn preview(note: &str) -> String {
    note.lines()
        .map(|line| line.trim().trim_start_matches('>').trim())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Collapsed: the note nearest the selected verse lies straight on top;
/// up to three more lie scattered underneath at their own angles.
fn notes_pile(
    app: &OmaBibleApp,
    notes: &[(usize, u64, String)],
    cx: &mut Context<OmaBibleApp>,
) -> AnyElement {
    let front = notes
        .iter()
        .find(|(verse, _, _)| *verse >= app.verse_index)
        .or(notes.last())
        .expect("notes is not empty");
    let behind: Vec<u64> = notes
        .iter()
        .filter(|(_, id, _)| *id != front.1)
        .map(|(_, id, _)| *id)
        .take(PILE_BEHIND)
        .collect();
    let fill = cx.theme().popover;
    let edge = cx.theme().border;

    let scattered = canvas(
        |_, _, _| (),
        move |bounds, (), window, _| {
            let centre = bounds.center();
            for (depth, id) in behind.iter().enumerate().rev() {
                let seed = *id as u32;
                // Alternate sides so the pile fans both ways.
                let side = if depth % 2 == 0 { 1.0 } else { -1.0 };
                let angle = side * (3.0 + 4.5 * unit(seed, 1)) * std::f32::consts::PI / 180.;
                let offset = point(
                    px(side * (4.0 + 8.0 * unit(seed, 2))),
                    px(-6.0 + 12.0 * unit(seed, 3)),
                );
                paint_card(
                    window,
                    centre + offset,
                    PILE_WIDTH - 8.,
                    PILE_CARD,
                    angle,
                    fill,
                    edge,
                );
            }
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full();

    let card = v_flex()
        .absolute()
        .left(px(0.))
        .top(px((PILE_HEIGHT - PILE_CARD) / 2.))
        .w(px(PILE_WIDTH))
        .h(px(PILE_CARD))
        .px(px(12.))
        .py(px(10.))
        .gap(px(3.))
        .rounded(px(12.))
        .bg(fill)
        .border_1()
        .border_color(edge)
        .shadow_md()
        .child(
            h_flex()
                .justify_between()
                .child(
                    h_flex()
                        .gap(px(5.))
                        .items_center()
                        .text_size(px(11.))
                        .text_color(cx.theme().accent)
                        .child(Icon::new(IconName::StickyNote).size(px(12.)))
                        .child(format!("Verse {}", front.0 + 1)),
                )
                .when(notes.len() > 1, |row| {
                    row.child(
                        div()
                            .px(px(6.))
                            .rounded_full()
                            .bg(cx.theme().muted)
                            .text_size(px(10.))
                            .text_color(cx.theme().muted_foreground)
                            .child(notes.len().to_string()),
                    )
                }),
        )
        .child(
            div()
                .text_size(px(12.))
                .line_height(px(17.))
                .line_clamp(3)
                .text_color(cx.theme().foreground)
                .child(preview(&front.2)),
        );

    div()
        .relative()
        .w(px(PILE_WIDTH))
        .h(px(PILE_HEIGHT))
        .child(scattered)
        .child(card)
        .into_any_element()
}

/// Expanded: every note in the chapter as its own card, in verse order.
fn notes_list(
    app: &OmaBibleApp,
    notes: &[(usize, u64, String)],
    cx: &mut Context<OmaBibleApp>,
) -> AnyElement {
    let heading = app
        .book()
        .zip(app.chapter())
        .map(|(book, chapter)| format!("{} {}", book.name, chapter.number))
        .unwrap_or_default();
    let popover = cx.theme().popover;
    let border = cx.theme().border;
    let accent = cx.theme().accent;

    let cards = notes.iter().map(|(verse, id, note)| {
        let (verse, id) = (*verse, *id);
        let current = verse == app.verse_index;
        v_flex()
            .id(SharedString::from(format!("notes-card-{id}")))
            .w_full()
            .flex_shrink_0()
            .gap(px(4.))
            .px(px(14.))
            .py(px(11.))
            .rounded(px(12.))
            .bg(popover)
            .border_1()
            .border_color(if current { accent.opacity(0.6) } else { border })
            .shadow_md()
            .cursor_pointer()
            .hover(move |style| style.border_color(accent.opacity(0.8)))
            .on_click(cx.listener(move |this, _, _, cx| this.open_chapter_note(verse, id, cx)))
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(accent)
                    .child(format!("Verse {}", verse + 1)),
            )
            .child(
                div()
                    .text_size(px(13.))
                    .line_height(px(19.))
                    .line_clamp(4)
                    .text_color(cx.theme().foreground)
                    .child(preview(note)),
            )
    });

    v_flex()
        .w(px(LIST_WIDTH))
        .gap(px(8.))
        .child(
            h_flex()
                .flex_shrink_0()
                .justify_between()
                .px(px(12.))
                .py(px(6.))
                .rounded_full()
                .bg(popover)
                .border_1()
                .border_color(border)
                .text_size(px(10.))
                .text_color(cx.theme().muted_foreground)
                .child(format!("NOTES · {}", heading.to_uppercase()))
                .child(notes.len().to_string()),
        )
        .child(
            v_flex()
                .id("notes-stack-list")
                .max_h(px(460.))
                .overflow_y_scroll()
                .gap(px(8.))
                .pb(px(6.))
                .children(cards),
        )
        .into_any_element()
}

/// A card turned by `angle` radians around `centre`: soft shadow, border,
/// then the opaque face.
fn paint_card(
    window: &mut Window,
    centre: Point<Pixels>,
    width: f32,
    height: f32,
    angle: f32,
    fill: Hsla,
    edge: Hsla,
) {
    let shadow = centre + point(px(0.), px(4.));
    let layers = [
        (shadow, 2.0, hsla(0., 0., 0., 0.22)),
        (centre, 0.0, edge),
        (centre, -1.0, fill),
    ];
    for (at, grow, color) in layers {
        let outline = rounded_rect(at, width + grow * 2., height + grow * 2., 12. + grow, angle);
        let mut builder = PathBuilder::fill();
        builder.add_polygon(&outline, true);
        if let Ok(path) = builder.build() {
            window.paint_path(path, color);
        }
    }
}

fn rounded_rect(
    centre: Point<Pixels>,
    width: f32,
    height: f32,
    radius: f32,
    angle: f32,
) -> Vec<Point<Pixels>> {
    let (hw, hh) = (width / 2., height / 2.);
    let r = radius.min(hw).min(hh);
    let corners = [
        (hw - r, hh - r, 0.0),
        (-hw + r, hh - r, 90.0),
        (-hw + r, -hh + r, 180.0),
        (hw - r, -hh + r, 270.0),
    ];
    let (sin, cos) = angle.sin_cos();
    let (cx, cy) = (f32::from(centre.x), f32::from(centre.y));
    let mut points = Vec::with_capacity(28);
    for (x, y, start) in corners {
        for step in 0..=6 {
            let a = (start + 15.0 * step as f32).to_radians();
            let (px_, py_) = (x + r * a.cos(), y + r * a.sin());
            points.push(point(
                px(cx + px_ * cos - py_ * sin),
                px(cy + px_ * sin + py_ * cos),
            ));
        }
    }
    points
}

/// Deterministic value in [0, 1) so the pile keeps its shape between frames.
fn unit(seed: u32, salt: u32) -> f32 {
    let mut x = seed.wrapping_mul(0x9E37_79B9) ^ salt.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    x = x.wrapping_mul(0x297A_2D39);
    x ^= x >> 15;
    x as f32 / (u32::MAX as f32 + 1.)
}
