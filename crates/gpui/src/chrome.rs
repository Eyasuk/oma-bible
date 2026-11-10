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

use std::f32::consts::{FRAC_PI_2, TAU};

use crate::app::OmaBibleApp;
use crate::tools::Tool;

/// Outer and inner radius of the wheel's ring; the inside is a hole.
const WHEEL_OUTER: f32 = 88.;
const WHEEL_INNER: f32 = 46.;
/// Width of the cut between two segments.
const WHEEL_CUT: f32 = 5.;
const WHEEL_ICON: f32 = 16.;
const WHEEL_LABEL_GAP: f32 = 10.;
const WHEEL_LABEL_HEIGHT: f32 = 28.;
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

/// Keep a wheel opened near a window edge fully on screen, label included.
pub fn clamp_wheel_center(position: Point<Pixels>, viewport: Size<Pixels>) -> Point<Pixels> {
    let half = px(WHEEL_OUTER + 8.);
    let below = px(WHEEL_OUTER + WHEEL_LABEL_GAP + WHEEL_LABEL_HEIGHT + 8.);
    let clamp = |value: Pixels, low: Pixels, high: Pixels, extent: Pixels| {
        if extent <= low + high {
            extent / 2.
        } else {
            value.clamp(low, extent - high)
        }
    };
    point(
        clamp(position.x, half, half, viewport.width),
        clamp(position.y, half, below, viewport.height),
    )
}

/// Where the wheel's centre is in the window.
fn wheel_center(app: &OmaBibleApp, window: &Window) -> Point<Pixels> {
    app.wheel_center.unwrap_or_else(|| {
        let viewport = window.viewport_size();
        point(viewport.width / 2., viewport.height / 2.)
    })
}

/// The tool under a window position, picked by angle like a pie menu, so
/// the whole segment is the target and not just its icon. The hole in the
/// middle and the space outside pick nothing.
fn tool_at(center: Point<Pixels>, position: Point<Pixels>) -> Option<Tool> {
    let dx = f32::from(position.x - center.x);
    let dy = f32::from(position.y - center.y);
    let distance = dx.hypot(dy);
    if distance < WHEEL_INNER * 0.6 || distance > WHEEL_OUTER + 10. {
        return None;
    }
    let count = Tool::WHEEL.len();
    let step = TAU / count as f32;
    // 0 at the top, growing clockwise.
    let angle = (dy.atan2(dx) + FRAC_PI_2).rem_euclid(TAU);
    let index = ((angle + step / 2.) / step) as usize % count;
    Tool::WHEEL.get(index).copied()
}

/// A ring of tools cut into segments, with a hole in the middle. The
/// pointer picks a segment by its angle; the keyboard circles it with
/// h/l, the arrows or Tab and runs it with Enter. A small label under the
/// ring names the picked tool and its key. Opens at the pointer on
/// right-click, or centred with Space.
pub fn render_wheel(app: &OmaBibleApp, cx: &mut Context<OmaBibleApp>) -> AnyElement {
    let (title, hint) = match app.wheel_hover {
        Some(tool) => (tool.label().to_string(), tool.key().to_string()),
        None => (
            app.current_reference()
                .map(|(_, book, chapter, verse)| format!("{book} {chapter}:{verse}"))
                .unwrap_or_else(|| "No verse".to_string()),
            "← → ↵".to_string(),
        ),
    };

    let size = WHEEL_OUTER * 2.;
    let count = Tool::WHEEL.len();
    let step = TAU / count as f32;
    let middle = (WHEEL_OUTER + WHEEL_INNER) / 2.;

    let segment = cx.theme().popover;
    let edge = cx.theme().border;
    let picked_fill = cx.theme().accent;
    let active_fill = cx.theme().selection;
    let picked = app
        .wheel_hover
        .and_then(|tool| Tool::WHEEL.iter().position(|item| *item == tool));
    let active: Vec<bool> = Tool::WHEEL
        .iter()
        .map(|tool| app.tool_active(*tool))
        .collect();

    let ring_active = active.clone();
    let ring = canvas(
        |_, _, _| (),
        move |bounds, (), window, _| {
            let center = bounds.center();
            // A soft shadow: the ring again, dark and a little lower.
            for (spread, alpha) in [(6., 0.10), (3., 0.14)] {
                if let Some(path) = annulus(
                    center + point(px(0.), px(spread * 0.6)),
                    WHEEL_INNER - spread * 0.3,
                    WHEEL_OUTER + spread,
                ) {
                    window.paint_path(path, black().opacity(alpha));
                }
            }
            for (index, &is_active) in ring_active.iter().enumerate() {
                let mid = step * index as f32 - FRAC_PI_2;
                let fill = if picked == Some(index) {
                    picked_fill
                } else if is_active {
                    active_fill
                } else {
                    segment
                };
                let (start, end) = (mid - step / 2., mid + step / 2.);
                if let Some(path) = sector(center, start, end, PathBuilder::fill()) {
                    window.paint_path(path, fill);
                }
                if picked != Some(index)
                    && let Some(path) = sector(center, start, end, PathBuilder::stroke(px(1.)))
                {
                    window.paint_path(path, edge);
                }
            }
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full();

    let icons = Tool::WHEEL.iter().enumerate().map(|(index, &tool)| {
        let angle = step * index as f32 - FRAC_PI_2;
        let x = WHEEL_OUTER + middle * angle.cos() - WHEEL_ICON / 2.;
        let y = WHEEL_OUTER + middle * angle.sin() - WHEEL_ICON / 2.;
        let color = if picked == Some(index) {
            cx.theme().accent_foreground
        } else if active[index] {
            cx.theme().accent
        } else {
            cx.theme().muted_foreground
        };
        div()
            .absolute()
            .left(px(x))
            .top(px(y))
            .size(px(WHEEL_ICON))
            .flex()
            .items_center()
            .justify_center()
            .text_color(color)
            .child(Icon::new(tool.icon()).size(px(WHEEL_ICON)))
    });

    let label = h_flex()
        .absolute()
        .top(px(size + WHEEL_LABEL_GAP))
        .left_0()
        .w(px(size))
        .justify_center()
        .child(
            h_flex()
                .h(px(WHEEL_LABEL_HEIGHT))
                .items_center()
                .gap(px(8.))
                .px(px(12.))
                .rounded_full()
                .bg(cx.theme().popover)
                .border_1()
                .border_color(cx.theme().border)
                .shadow_md()
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(cx.theme().foreground)
                        .child(title),
                )
                .child(
                    div()
                        .px(px(5.))
                        .rounded(px(4.))
                        .bg(cx.theme().muted)
                        .text_size(px(10.))
                        .text_color(cx.theme().muted_foreground)
                        .child(hint),
                ),
        );

    let wheel = div()
        .id("action-wheel")
        .relative()
        .flex_shrink_0()
        .size(px(size))
        .cursor_pointer()
        .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
            let next = tool_at(wheel_center(this, window), event.position);
            if next != this.wheel_hover {
                this.wheel_hover = next;
                cx.notify();
            }
        }))
        .on_click(cx.listener(|this, event: &ClickEvent, window, cx| {
            cx.stop_propagation();
            // The hole in the middle is a way out, like the scrim.
            match tool_at(wheel_center(this, window), event.position()) {
                Some(tool) => this.run_tool(tool, window, cx),
                None => this.close_overlay(cx),
            }
        }))
        .child(ring)
        .children(icons)
        .child(label);

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
                .left(at.x - px(WHEEL_OUTER))
                .top(at.y - px(WHEEL_OUTER))
                .child(wheel),
        ),
        None => scrim.flex().items_center().justify_center().child(wheel),
    };
    scrim.into_any_element()
}

/// One ring segment from angle `start` to `end` (radians, clockwise from
/// the x axis), inset so neighbouring segments leave an even cut.
fn sector(
    center: Point<Pixels>,
    start: f32,
    end: f32,
    mut builder: PathBuilder,
) -> Option<Path<Pixels>> {
    let cut = WHEEL_CUT / 2.;
    let steps = 14;
    let at = |radius: f32, angle: f32| {
        center + point(px(radius * angle.cos()), px(radius * angle.sin()))
    };
    let (outer_start, outer_end) = (start + cut / WHEEL_OUTER, end - cut / WHEEL_OUTER);
    let (inner_start, inner_end) = (start + cut / WHEEL_INNER, end - cut / WHEEL_INNER);
    let mut points = Vec::with_capacity(steps * 2 + 2);
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        points.push(at(WHEEL_OUTER, outer_start + (outer_end - outer_start) * t));
    }
    for i in (0..=steps).rev() {
        let t = i as f32 / steps as f32;
        points.push(at(WHEEL_INNER, inner_start + (inner_end - inner_start) * t));
    }
    builder.add_polygon(&points, true);
    builder.build().ok()
}

/// A full ring between two radii, used for the shadow.
fn annulus(center: Point<Pixels>, inner: f32, outer: f32) -> Option<Path<Pixels>> {
    let steps = 64;
    let at = |radius: f32, angle: f32| {
        center + point(px(radius * angle.cos()), px(radius * angle.sin()))
    };
    let mut builder = PathBuilder::fill();
    let mut points: Vec<Point<Pixels>> = (0..=steps)
        .map(|i| at(outer, TAU * i as f32 / steps as f32))
        .collect();
    points.extend(
        (0..=steps)
            .rev()
            .map(|i| at(inner, TAU * i as f32 / steps as f32)),
    );
    builder.add_polygon(&points, true);
    builder.build().ok()
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

#[cfg(test)]
mod wheel_tests {
    // Not `super::*`: GPUI's own `test` macro would shadow the std one.
    use super::{Tool, WHEEL_INNER, WHEEL_OUTER, tool_at};
    use gpui_kit::{point, px};

    #[test]
    fn segments_are_picked_by_angle_clockwise_from_the_top() {
        let center = point(px(200.), px(200.));
        let middle = (WHEEL_OUTER + WHEEL_INNER) / 2.;
        let step = std::f32::consts::TAU / Tool::WHEEL.len() as f32;
        for (index, tool) in Tool::WHEEL.iter().enumerate() {
            // The middle of each segment, and a little either side of it.
            for nudge in [-0.3, 0., 0.3] {
                let angle = step * (index as f32 + nudge) - std::f32::consts::FRAC_PI_2;
                let at = point(
                    px(200. + middle * angle.cos()),
                    px(200. + middle * angle.sin()),
                );
                assert_eq!(tool_at(center, at), Some(*tool), "segment {index}");
            }
        }
    }

    #[test]
    fn the_hole_and_the_outside_pick_nothing() {
        let center = point(px(0.), px(0.));
        assert_eq!(tool_at(center, point(px(0.), px(5.))), None);
        assert_eq!(tool_at(center, point(px(WHEEL_OUTER + 40.), px(0.))), None);
    }
}
