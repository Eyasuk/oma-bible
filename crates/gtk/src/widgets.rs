use gtk::prelude::*;
use oma_bible_core::theme::Theme;

/// Widgets owned by the main window.
pub(crate) struct Widgets {
    pub window: adw::ApplicationWindow,
    pub translation_dropdown: gtk::DropDown,
    pub books: gtk::ListBox,
    pub chapters: gtk::ListBox,
    pub verses: gtk::ListBox,
    pub heading: gtk::Label,
    pub status: gtk::Label,
    pub theme_label: gtk::Label,
    pub provider: gtk::CssProvider,
    pub split: adw::OverlaySplitView,
    pub bookmarks_list: gtk::ListBox,
    pub bookmarks_title: gtk::Label,
    pub search_entry: gtk::SearchEntry,
    pub reading_scroll: gtk::ScrolledWindow,
    pub search_button: gtk::Button,
    pub bookmarks_button: gtk::Button,
    pub download_button: gtk::Button,
    pub settings_button: gtk::Button,
    pub help_button: gtk::Button,
    pub bookmarks_remove: gtk::Button,
    pub bookmarks_note: gtk::Button,
    pub bookmarks_clear: gtk::Button,
    pub settings: SettingsWidgets,
    pub download: DownloadWidgets,
}

/// The preferences window.
pub(crate) struct SettingsWidgets {
    pub window: adw::PreferencesWindow,
    pub theme_dropdown: Option<gtk::DropDown>,
    pub verse_switch: adw::SwitchRow,
    pub font_spin: gtk::SpinButton,
    pub bookmarks_row: adw::ActionRow,
    pub add_button: gtk::Button,
    pub clear_button: gtk::Button,
}

/// The translation browser.
pub(crate) struct DownloadWidgets {
    pub window: gtk::Window,
    pub search: gtk::SearchEntry,
    pub language: gtk::DropDown,
    pub list: gtk::ListBox,
    pub spinner: gtk::Spinner,
    pub progress: gtk::Label,
    pub cancel: gtk::Button,
    pub refresh: gtk::Button,
}

pub(crate) fn icon_button(icon: &str, tooltip: &str) -> gtk::Button {
    let button = gtk::Button::from_icon_name(icon);
    button.set_tooltip_text(Some(tooltip));
    button.set_can_focus(false);
    button.add_css_class("oma-icon-button");
    button
}

pub(crate) fn theme_css(theme: &Theme, font_size: u32) -> String {
    let accent = theme.accent.to_hex();
    let selection = theme.selection.to_hex();
    let muted = theme.muted.to_hex();
    let background = theme.background.to_hex();
    let dark_background = theme.dark_background.to_hex();
    let lighter_background = theme.lighter_background.to_hex();
    let foreground = theme.foreground.to_hex();
    let bright_foreground = theme.bright_foreground.to_hex();

    format!(
        r#"
.oma-window, .oma-root {{
    background-color: {background};
    color: {foreground};
}}
headerbar {{
    background-color: {dark_background};
    color: {foreground};
    box-shadow: none;
}}
.oma-topbar {{
    background-color: {dark_background};
    border-bottom: 1px solid {muted};
    padding: 10px 16px;
}}
.oma-title {{
    color: {accent};
    font-family: "IBM Plex Mono", "Noto Sans Mono", monospace;
    font-size: 12px;
    font-weight: 700;
}}
.oma-badge, .oma-muted {{
    color: {muted};
    font-family: "IBM Plex Mono", "Noto Sans Mono", monospace;
    font-size: 11px;
}}
.oma-sidebar {{
    background-color: {dark_background};
    border-right: 1px solid {muted};
}}
.oma-sidebar-panel {{
    background-color: {dark_background};
    border-left: 1px solid {muted};
}}
.oma-sidebar-header {{
    padding: 12px 14px 6px 14px;
}}
.oma-section {{
    color: {muted};
    font-family: "IBM Plex Mono", "Noto Sans Mono", monospace;
    font-size: 10px;
    font-weight: 700;
}}
.oma-list {{
    background-color: transparent;
    color: {foreground};
}}
.oma-list row {{
    color: {foreground};
    margin: 1px 8px;
    padding: 6px 10px;
    border-radius: 6px;
}}
.oma-list row:hover {{
    background-color: {lighter_background};
}}
.oma-list row:selected {{
    background-color: {selection};
    color: {bright_foreground};
}}
.oma-scroll {{
    background-color: transparent;
}}
.oma-scroll undershoot, .oma-scroll overshoot {{
    background-color: transparent;
}}
.oma-paned > separator {{
    background-color: transparent;
    min-width: 0px;
    min-height: 0px;
}}
.oma-reading {{
    background-color: {background};
}}
.oma-heading {{
    color: {accent};
    font-family: "Noto Serif", "Noto Serif Ethiopic", serif;
    font-size: 26px;
    font-weight: 700;
}}
.oma-verses {{
    background-color: transparent;
}}
.oma-verses row, .oma-verse-row {{
    background-color: transparent;
    border-radius: 8px;
    padding: 4px 10px;
}}
.oma-verses row:hover {{
    background-color: {lighter_background};
}}
.oma-verses row:selected {{
    background-color: {selection};
}}
.oma-verse-number {{
    color: {muted};
    font-family: "IBM Plex Mono", "Noto Sans Mono", monospace;
    font-size: 11px;
    font-weight: 700;
    margin-top: 4px;
}}
.oma-verse-number-bookmarked {{
    color: {accent};
}}
.oma-verse-text {{
    color: {foreground};
    font-family: "Noto Serif", "Noto Serif Ethiopic", serif;
    font-size: {font_size}px;
}}
.oma-footer {{
    background-color: {dark_background};
    border-top: 1px solid {muted};
    padding: 8px 16px;
}}
.oma-dropdown > button {{
    background-color: {lighter_background};
    background-image: none;
    color: {foreground};
    border: 1px solid {muted};
    border-radius: 6px;
    box-shadow: none;
    padding: 4px 10px;
    min-height: 0px;
}}
.oma-dropdown > button:hover {{
    border-color: {accent};
}}
.oma-dropdown popover > contents {{
    background-color: {dark_background};
    color: {foreground};
}}
.oma-dropdown popover row:selected {{
    background-color: {selection};
    color: {bright_foreground};
}}
.oma-icon-button {{
    background-color: transparent;
    background-image: none;
    border: 1px solid transparent;
    box-shadow: none;
    color: {muted};
    padding: 4px 6px;
    min-height: 0px;
}}
.oma-icon-button:hover {{
    background-color: {lighter_background};
    color: {foreground};
    border-color: {muted};
}}
.oma-search-entry {{
    background-color: {lighter_background};
    color: {foreground};
    border: 1px solid {muted};
    border-radius: 6px;
    min-height: 0px;
}}
.oma-bookmark-row {{
    border-bottom: 1px solid {lighter_background};
}}
.oma-bookmark-content {{
    padding: 8px 14px;
}}
.oma-bookmark-reference {{
    color: {foreground};
    font-family: "Noto Serif", "Noto Serif Ethiopic", serif;
    font-size: 15px;
    font-weight: 700;
}}
.oma-download-row {{
    border-bottom: 1px solid {lighter_background};
}}
.oma-download-content {{
    padding: 9px 12px;
}}
.oma-download-title {{
    color: {foreground};
    font-size: 14px;
    font-weight: 700;
}}
.oma-help {{
    color: {foreground};
    font-family: "IBM Plex Mono", "Noto Sans Mono", monospace;
    font-size: 12px;
}}
.oma-note-entry {{
    background-color: {lighter_background};
    color: {foreground};
    border: 1px solid {muted};
    border-radius: 6px;
}}
"#
    )
}
