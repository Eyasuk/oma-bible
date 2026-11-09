//! Reader chrome that floats over the page: the shortcut dock, the
//! right-click action wheel, and the stack of chapter notes.
//!
//! Each piece stays quiet until the pointer reaches it — dimmed, compact, or
//! collapsed — so the reading column remains the only loud thing on screen.

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

const STACK_WIDTH: f32 = 168.;
const STACK_CARD: f32 = 78.;
const STACK_STEP: f32 = 6.;

/// The open chapter's notes as a pile of cards at the middle of the right
/// edge (the left edge when the dock is there). Hovering fans the pile out
/// into a list; clicking a card selects the verse and opens the note.
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

/// Collapsed: the note nearest the selected verse on top, up to two more
/// cards peeking out underneath.
fn notes_pile(
    app: &OmaBibleApp,
    notes: &[(usize, String)],
    cx: &mut Context<OmaBibleApp>,
) -> AnyElement {
    let front = notes
        .iter()
        .find(|(verse, _)| *verse >= app.verse_index)
        .or(notes.last())
        .expect("notes is not empty");
    let layers = notes.len().min(3);
    let popover = cx.theme().popover;
    let border = cx.theme().border;

    let mut pile = div()
        .relative()
        .w(px(STACK_WIDTH))
        .h(px(STACK_CARD + STACK_STEP * (layers - 1) as f32))
        .opacity(0.88);
    for depth in (1..layers).rev() {
        let inset = px(STACK_STEP * depth as f32);
        pile = pile.child(
            div()
                .absolute()
                .top(inset)
                .left(inset)
                .right(inset)
                .h(px(STACK_CARD))
                .rounded(px(12.))
                .bg(popover)
                .border_1()
                .border_color(border)
                .opacity(1. - 0.3 * depth as f32),
        );
    }

    let card = v_flex()
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .h(px(STACK_CARD))
        .px(px(12.))
        .py(px(9.))
        .gap(px(2.))
        .rounded(px(12.))
        .bg(popover)
        .border_1()
        .border_color(border)
        .shadow_md()
        .child(
            h_flex()
                .justify_between()
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(cx.theme().accent)
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
                .line_height(px(16.))
                .line_clamp(2)
                .text_color(cx.theme().foreground)
                .child(front.1.clone()),
        );

    pile.child(card).into_any_element()
}

/// Expanded: every note in the chapter, newest verse order, scrollable.
fn notes_list(
    app: &OmaBibleApp,
    notes: &[(usize, String)],
    cx: &mut Context<OmaBibleApp>,
) -> AnyElement {
    let heading = app
        .book()
        .zip(app.chapter())
        .map(|(book, chapter)| format!("{} {}", book.name, chapter.number))
        .unwrap_or_default();
    let selection = cx.theme().selection;
    let hover = cx.theme().muted;

    let cards = notes.iter().map(|(verse, note)| {
        let verse = *verse;
        let card = v_flex()
            .id(SharedString::from(format!("notes-card-{verse}")))
            .w_full()
            .gap(px(3.))
            .px(px(10.))
            .py(px(8.))
            .rounded(px(10.))
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, _, cx| this.open_chapter_note(verse, cx)))
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(cx.theme().accent)
                    .child(format!("Verse {}", verse + 1)),
            )
            .child(
                div()
                    .text_size(px(13.))
                    .line_height(px(19.))
                    .line_clamp(3)
                    .text_color(cx.theme().foreground)
                    .child(note.clone()),
            );
        if verse == app.verse_index {
            card.bg(selection)
        } else {
            card.hover(move |style| style.bg(hover))
        }
    });

    v_flex()
        .w(px(288.))
        .p(px(6.))
        .gap(px(4.))
        .rounded(px(16.))
        .bg(cx.theme().popover)
        .border_1()
        .border_color(cx.theme().border)
        .shadow_xl()
        .child(
            h_flex()
                .justify_between()
                .px(px(10.))
                .pt(px(6.))
                .pb(px(2.))
                .text_size(px(11.))
                .text_color(cx.theme().muted_foreground)
                .child(format!("NOTES · {}", heading.to_uppercase()))
                .child(notes.len().to_string()),
        )
        .child(
            v_flex()
                .id("notes-stack-list")
                .max_h(px(420.))
                .overflow_y_scroll()
                .gap(px(2.))
                .children(cards),
        )
        .into_any_element()
}
