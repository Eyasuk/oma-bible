//! Bridge the Omarchy theme palette into gpui-kit's global theme.
//!
//! `oma-bible-core` owns Omarchy discovery and the palette used by the TUI.
//! gpui-kit draws its controls and text from a global `Theme`, so we translate
//! the active core theme into the tokens the components read. On Omarchy the
//! active theme is always the one Omarchy is using; elsewhere the app chooses a
//! light or dark palette and lets the theme selector override it.

use gpui_kit::component::{ActiveTheme, Theme as GpuiTheme, ThemeMode};
use gpui_kit::{App, Hsla, Window, WindowAppearance, px, rgb};
use oma_bible_core::storage::HighlightColor;
use oma_bible_core::theme::{Rgb, Theme, ThemeMode as CoreThemeMode};

/// Whether the desktop is currently in a dark appearance.
pub fn system_is_dark(window: &Window) -> bool {
    matches!(
        window.appearance(),
        WindowAppearance::Dark | WindowAppearance::VibrantDark
    )
}

/// A core palette colour as a gpui-kit colour.
pub fn color(value: Rgb) -> Hsla {
    rgb(((value.red as u32) << 16) | ((value.green as u32) << 8) | value.blue as u32).into()
}

/// The palette colour used to draw a verse highlight.
pub fn highlight(value: HighlightColor, cx: &App) -> Hsla {
    let theme = cx.theme();
    match value {
        HighlightColor::Yellow => theme.yellow,
        HighlightColor::Green => theme.green,
        HighlightColor::Blue => theme.blue,
        HighlightColor::Pink => theme.magenta,
    }
}

/// Install the active core palette into gpui-kit.
///
/// This is deliberately done as a full palette replacement: the reader and
/// every native control then share the same Omarchy colours, instead of the
/// reader looking themed while menus and inputs keep gpui-kit's default blue.
pub fn apply(theme: &Theme, cx: &mut App) {
    let mode = match theme.mode {
        CoreThemeMode::Dark => ThemeMode::Dark,
        CoreThemeMode::Light => ThemeMode::Light,
    };
    GpuiTheme::change(mode, None, cx);

    let background = color(theme.background);
    let surface = color(theme.lighter_background);
    let deep_surface = color(theme.dark_background);
    let foreground = color(theme.foreground);
    let muted = color(theme.muted);
    let border = color(theme.selection);
    let accent = color(theme.accent);
    let accent_foreground = contrast_on(theme.accent, theme);
    let primary_foreground = accent_foreground;

    let red = color(theme.red);
    let green = color(theme.green);
    let yellow = color(theme.yellow);
    let blue = color(theme.blue);
    let cyan = color(theme.cyan);
    let red_foreground = contrast_on(theme.red, theme);
    let green_foreground = contrast_on(theme.green, theme);
    let yellow_foreground = contrast_on(theme.yellow, theme);
    let cyan_foreground = contrast_on(theme.cyan, theme);

    GpuiTheme::update(cx, |theme| {
        theme.radius = px(10.);
        theme.radius_lg = px(18.);
        theme.font_size = px(16.);

        theme.colors.background = background;
        theme.colors.foreground = foreground;
        theme.colors.border = border;
        theme.colors.muted = surface;
        theme.colors.muted_foreground = muted;
        theme.colors.accent = accent;
        theme.colors.accent_foreground = accent_foreground;
        theme.colors.secondary = surface;
        theme.colors.secondary_foreground = foreground;
        theme.colors.secondary_hover = border;
        theme.colors.secondary_active = border;
        theme.colors.primary = accent;
        theme.colors.primary_foreground = primary_foreground;
        theme.colors.primary_hover = accent;
        theme.colors.primary_active = accent;
        theme.colors.selection = border;
        theme.colors.ring = accent;
        theme.colors.caret = accent;
        theme.colors.input = surface;
        theme.colors.popover = surface;
        theme.colors.popover_foreground = foreground;
        theme.colors.list = background;
        theme.colors.list_even = background;
        theme.colors.list_head = surface;
        theme.colors.list_hover = border;
        theme.colors.list_active = border;
        theme.colors.list_active_border = accent;
        theme.colors.button = surface;
        theme.colors.button_foreground = foreground;
        theme.colors.button_hover = border;
        theme.colors.button_active = border;
        theme.colors.button_secondary = surface;
        theme.colors.button_secondary_foreground = foreground;
        theme.colors.button_secondary_hover = border;
        theme.colors.button_secondary_active = border;
        theme.colors.button_primary = accent;
        theme.colors.button_primary_foreground = primary_foreground;
        theme.colors.button_primary_hover = accent;
        theme.colors.button_primary_active = accent;
        theme.colors.group_box = surface;
        theme.colors.group_box_foreground = foreground;
        theme.colors.sidebar = deep_surface;
        theme.colors.sidebar_foreground = foreground;
        theme.colors.sidebar_border = border;
        theme.colors.sidebar_accent = border;
        theme.colors.sidebar_accent_foreground = foreground;
        theme.colors.sidebar_primary = accent;
        theme.colors.sidebar_primary_foreground = primary_foreground;
        theme.colors.tab_bar = background;
        theme.colors.tab = surface;
        theme.colors.tab_active = border;
        theme.colors.tab_active_foreground = foreground;
        theme.colors.tab_foreground = muted;
        theme.colors.scrollbar = background;
        theme.colors.scrollbar_thumb = border;
        theme.colors.scrollbar_thumb_hover = muted;
        theme.colors.danger = red;
        theme.colors.danger_hover = red;
        theme.colors.danger_active = red;
        theme.colors.danger_foreground = red_foreground;
        theme.colors.button_danger = red;
        theme.colors.button_danger_foreground = red_foreground;
        theme.colors.button_danger_hover = red;
        theme.colors.button_danger_active = red;
        theme.colors.success = green;
        theme.colors.success_hover = green;
        theme.colors.success_active = green;
        theme.colors.success_foreground = green_foreground;
        theme.colors.button_success = green;
        theme.colors.button_success_foreground = green_foreground;
        theme.colors.warning = yellow;
        theme.colors.warning_hover = yellow;
        theme.colors.warning_active = yellow;
        theme.colors.warning_foreground = yellow_foreground;
        theme.colors.button_warning = yellow;
        theme.colors.button_warning_foreground = yellow_foreground;
        theme.colors.info = cyan;
        theme.colors.info_hover = cyan;
        theme.colors.info_active = cyan;
        theme.colors.info_foreground = cyan_foreground;
        theme.colors.button_info = cyan;
        theme.colors.button_info_foreground = cyan_foreground;
        theme.colors.link = blue;
        theme.colors.link_hover = cyan;
        theme.colors.link_active = cyan;
        theme.colors.skeleton = deep_surface;
        theme.colors.switch = accent;
        theme.colors.switch_thumb = background;
        theme.colors.progress_bar = accent;
        theme.colors.slider_bar = border;
        theme.colors.slider_thumb = accent;
        theme.colors.drop_target = border;
        theme.colors.drag_border = accent;
        theme.colors.caret = accent;
    });
}

/// Choose black or white text for a coloured chip.
fn contrast_on(value: Rgb, theme: &Theme) -> Hsla {
    let luminance = relative_luminance(value);
    if luminance > 0.58 {
        color(theme.dark_background)
    } else {
        color(theme.bright_foreground)
    }
}

fn relative_luminance(value: Rgb) -> f32 {
    fn channel(value: u8) -> f32 {
        let value = value as f32 / 255.0;
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    }

    0.2126 * channel(value.red) + 0.7152 * channel(value.green) + 0.0722 * channel(value.blue)
}

/// The theme currently installed in gpui-kit, used by tests or diagnostics.
#[allow(dead_code)]
pub fn active_is_dark(cx: &App) -> bool {
    cx.theme().is_dark()
}
