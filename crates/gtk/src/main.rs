use std::{rc::Rc, sync::mpsc, time::Duration};

use adw::prelude::*;
use gtk::{gdk, glib};
use oma_bible_core::{
    bible::Library,
    storage::{Bookmarks, Settings},
    theme::ThemeCatalog,
    user_data_dir,
};

mod app;
mod widgets;

use app::{Initial, Ui};
use widgets::{DownloadWidgets, SettingsWidgets, Widgets, icon_button};

fn main() -> glib::ExitCode {
    let application = adw::Application::builder()
        .application_id("dev.omarchy.OmaBible")
        .build();

    application.connect_activate(build_ui);
    application.run()
}

fn build_ui(application: &adw::Application) {
    let mut themes = ThemeCatalog::load();
    let settings = Settings::load();
    if !themes.is_on_omarchy()
        && let Some(slug) = settings.theme.as_deref()
    {
        themes.select_slug(slug);
    }

    let initial = Initial {
        library: Library::load_default(),
        settings,
        bookmarks: Bookmarks::load(),
        themes,
    };

    let (worker_tx, worker_rx) = mpsc::channel();
    let widgets = build_widgets(application, &initial.themes);
    let ui = Ui::new(initial, worker_tx, worker_rx, widgets);

    if let Some(display) = gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &ui.widgets.provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }

    wire_signals(&ui);
    ui.initial_load();

    if let Some(notice) = ui.prepare_fonts() {
        ui.widgets.status.set_text(&notice);
    }

    let has_translations = ui.has_translations();
    let needs_theme_picker = ui.needs_theme_picker();

    ui.widgets.window.present();
    ui.widgets.books.grab_focus();

    if !has_translations {
        ui.show_download();
        ui.widgets.download.progress.set_text(&format!(
            "No translations installed. Add one from the Bible API. Downloads: {}",
            user_data_dir().display()
        ));
    } else if needs_theme_picker {
        // Without Omarchy there is no system theme to follow, so offer the
        // discovered themes on first run.
        ui.show_settings();
    }
}

fn build_widgets(application: &adw::Application, themes: &ThemeCatalog) -> Widgets {
    let window = adw::ApplicationWindow::builder()
        .application(application)
        .title("Oma Bible")
        .default_width(1180)
        .default_height(820)
        .build();
    window.add_css_class("oma-window");

    let translation_dropdown = gtk::DropDown::from_strings(&[]);
    translation_dropdown.add_css_class("oma-dropdown");
    translation_dropdown.set_tooltip_text(Some("Translation (t)"));
    translation_dropdown.set_width_request(220);

    let title = gtk::Label::new(Some("OMA BIBLE"));
    title.add_css_class("oma-title");

    let search_entry = gtk::SearchEntry::new();
    search_entry.add_css_class("oma-search-entry");
    search_entry.set_placeholder_text(Some("Search this chapter…"));
    search_entry.set_width_chars(20);
    search_entry.set_visible(false);

    let title_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    title_box.append(&title);
    title_box.append(&translation_dropdown);
    title_box.append(&search_entry);

    let header = gtk::HeaderBar::new();
    header.add_css_class("oma-topbar");
    header.set_title_widget(Some(&title_box));

    let search_button = icon_button("edit-find-symbolic", "Search (/)");
    let bookmarks_button = icon_button("user-bookmarks-symbolic", "Bookmarks & study (S)");
    let download_button = icon_button("cloud-download-symbolic", "Add a translation (D)");
    let settings_button = icon_button("emblem-system-symbolic", "Settings (T)");
    let help_button = icon_button("help-about-symbolic", "Keyboard shortcuts (?)");

    header.pack_end(&help_button);
    header.pack_end(&settings_button);
    header.pack_end(&download_button);
    header.pack_end(&bookmarks_button);
    header.pack_end(&search_button);

    let books = gtk::ListBox::new();
    books.add_css_class("oma-list");
    books.set_selection_mode(gtk::SelectionMode::Single);

    let chapters = gtk::ListBox::new();
    chapters.add_css_class("oma-list");
    chapters.set_selection_mode(gtk::SelectionMode::Single);

    let books_section = sidebar_section("BOOKS", &books);
    books_section.set_vexpand(true);
    let chapters_section = sidebar_section("CHAPTERS", &chapters);
    chapters_section.set_vexpand(true);

    let side_paned = gtk::Paned::new(gtk::Orientation::Vertical);
    side_paned.add_css_class("oma-paned");
    side_paned.set_start_child(Some(&books_section));
    side_paned.set_end_child(Some(&chapters_section));
    side_paned.set_position(380);

    let sidebar = gtk::Box::new(gtk::Orientation::Vertical, 0);
    sidebar.add_css_class("oma-sidebar");
    sidebar.set_width_request(260);
    sidebar.append(&side_paned);

    let heading = gtk::Label::builder().xalign(0.0).build();
    heading.add_css_class("oma-heading");

    let verses = gtk::ListBox::new();
    verses.add_css_class("oma-verses");
    verses.set_selection_mode(gtk::SelectionMode::Single);

    let reading_box = gtk::Box::new(gtk::Orientation::Vertical, 10);
    reading_box.set_margin_top(24);
    reading_box.set_margin_bottom(40);
    reading_box.set_margin_start(36);
    reading_box.set_margin_end(36);
    reading_box.append(&heading);
    reading_box.append(&verses);

    let clamp = adw::Clamp::builder()
        .maximum_size(760)
        .tightening_threshold(640)
        .child(&reading_box)
        .build();
    clamp.set_hexpand(true);

    let reading_scroll = gtk::ScrolledWindow::builder()
        .child(&clamp)
        .vexpand(true)
        .hexpand(true)
        .build();
    reading_scroll.add_css_class("oma-scroll");
    reading_scroll.add_css_class("oma-reading");

    let status = gtk::Label::builder().xalign(0.0).hexpand(true).build();
    status.add_css_class("oma-muted");

    let footer = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    footer.add_css_class("oma-footer");
    footer.append(&status);

    let reading = gtk::Box::new(gtk::Orientation::Vertical, 0);
    reading.append(&reading_scroll);
    reading.append(&footer);

    let bookmarks_title = gtk::Label::builder()
        .label("BOOKMARKS")
        .xalign(0.0)
        .hexpand(true)
        .build();
    bookmarks_title.add_css_class("oma-section");

    let bookmarks_clear = icon_button("edit-clear-all-symbolic", "Clear all bookmarks");
    let bookmarks_remove = icon_button("user-trash-symbolic", "Remove bookmark (x)");
    let bookmarks_note = icon_button("document-edit-symbolic", "Edit note (M)");

    let panel_header = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    panel_header.add_css_class("oma-sidebar-header");
    panel_header.append(&bookmarks_title);
    panel_header.append(&bookmarks_clear);
    panel_header.append(&bookmarks_remove);
    panel_header.append(&bookmarks_note);

    let bookmarks_list = gtk::ListBox::new();
    bookmarks_list.add_css_class("oma-list");
    bookmarks_list.set_selection_mode(gtk::SelectionMode::Single);

    let bookmarks_scroll = gtk::ScrolledWindow::builder()
        .child(&bookmarks_list)
        .vexpand(true)
        .hexpand(true)
        .build();
    bookmarks_scroll.add_css_class("oma-scroll");

    let panel = gtk::Box::new(gtk::Orientation::Vertical, 0);
    panel.add_css_class("oma-sidebar-panel");
    panel.set_width_request(330);
    panel.append(&panel_header);
    panel.append(&bookmarks_scroll);

    let split = adw::OverlaySplitView::builder()
        .content(&reading)
        .sidebar(&panel)
        .sidebar_position(gtk::PackType::End)
        .show_sidebar(false)
        .build();
    split.set_hexpand(true);
    split.set_vexpand(true);

    let body = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    body.append(&sidebar);
    body.append(&split);

    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    root.add_css_class("oma-root");
    root.append(&body);

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&root));
    window.set_content(Some(&toolbar));

    let settings = build_settings(&window, themes);
    let download = build_download(&window);

    Widgets {
        window,
        translation_dropdown,
        books,
        chapters,
        verses,
        heading,
        status,
        provider: gtk::CssProvider::new(),
        split,
        bookmarks_list,
        bookmarks_title,
        search_entry,
        reading_scroll,
        search_button,
        bookmarks_button,
        download_button,
        settings_button,
        help_button,
        bookmarks_remove,
        bookmarks_note,
        bookmarks_clear,
        settings,
        download,
    }
}

fn sidebar_section(title: &str, list: &gtk::ListBox) -> gtk::Box {
    let label = gtk::Label::builder().label(title).xalign(0.0).build();
    label.add_css_class("oma-section");

    let header = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    header.add_css_class("oma-sidebar-header");
    header.append(&label);

    let scrolled = gtk::ScrolledWindow::builder()
        .child(list)
        .vexpand(true)
        .hexpand(true)
        .build();
    scrolled.add_css_class("oma-scroll");

    let section = gtk::Box::new(gtk::Orientation::Vertical, 0);
    section.append(&header);
    section.append(&scrolled);
    section
}

fn build_settings(window: &adw::ApplicationWindow, themes: &ThemeCatalog) -> SettingsWidgets {
    let settings_window = adw::PreferencesWindow::builder()
        .transient_for(window)
        .modal(false)
        .hide_on_close(true)
        .title("Settings")
        .default_width(560)
        .default_height(640)
        .build();
    settings_window.add_css_class("oma-window");

    let page = adw::PreferencesPage::new();

    let appearance = adw::PreferencesGroup::builder().title("Appearance").build();

    let theme_dropdown = if themes.is_on_omarchy() {
        let row = adw::ActionRow::builder()
            .title("Theme")
            .subtitle("Following the Omarchy system theme")
            .build();
        appearance.add(&row);
        None
    } else {
        let names = themes
            .themes()
            .iter()
            .map(|theme| theme.name.clone())
            .collect::<Vec<_>>();
        let names = names.iter().map(String::as_str).collect::<Vec<_>>();
        let dropdown = gtk::DropDown::from_strings(&names);
        dropdown.add_css_class("oma-dropdown");
        dropdown.set_selected(themes.current_index().unwrap_or(0) as u32);

        let row = adw::ActionRow::builder()
            .title("Theme")
            .subtitle("Applied to the reader and remembered")
            .build();
        row.add_suffix(&dropdown);
        row.set_activatable_widget(Some(&dropdown));
        appearance.add(&row);
        Some(dropdown)
    };

    let verse_switch = adw::SwitchRow::builder().title("Verse numbers").build();
    appearance.add(&verse_switch);

    let font_spin = gtk::SpinButton::with_range(12.0, 30.0, 1.0);
    font_spin.set_width_chars(4);
    font_spin.set_numeric(true);
    let font_row = adw::ActionRow::builder().title("Reading text size").build();
    font_row.add_suffix(&font_spin);
    font_row.set_activatable_widget(Some(&font_spin));
    appearance.add(&font_row);

    page.add(&appearance);

    let library = adw::PreferencesGroup::builder().title("Library").build();

    let add_button = gtk::Button::with_label("Add");
    add_button.add_css_class("suggested-action");
    let add_row = adw::ActionRow::builder()
        .title("Add a translation")
        .subtitle("Download from bible.helloao.org")
        .build();
    add_row.add_suffix(&add_button);
    add_row.set_activatable_widget(Some(&add_button));
    library.add(&add_row);

    let bookmarks_row = adw::ActionRow::builder().title("Bookmarks").build();
    let clear_button = gtk::Button::with_label("Clear");
    clear_button.add_css_class("destructive-action");
    bookmarks_row.add_suffix(&clear_button);
    library.add(&bookmarks_row);

    page.add(&library);

    settings_window.add(&page);

    SettingsWidgets {
        window: settings_window,
        theme_dropdown,
        verse_switch,
        font_spin,
        bookmarks_row,
        add_button,
        clear_button,
    }
}

fn build_download(window: &adw::ApplicationWindow) -> DownloadWidgets {
    let download_window = gtk::Window::builder()
        .title("Add a translation")
        .transient_for(window)
        .modal(false)
        .hide_on_close(true)
        .default_width(720)
        .default_height(660)
        .build();
    download_window.add_css_class("oma-window");

    let header = gtk::HeaderBar::new();
    header.add_css_class("oma-topbar");
    let title = gtk::Label::new(Some("BIBLE API"));
    title.add_css_class("oma-title");
    header.set_title_widget(Some(&title));
    let refresh = icon_button("view-refresh-symbolic", "Refresh catalogue (r)");
    header.pack_end(&refresh);
    download_window.set_titlebar(Some(&header));

    let search = gtk::SearchEntry::new();
    search.add_css_class("oma-search-entry");
    search.set_placeholder_text(Some("Search translations…"));
    search.set_hexpand(true);

    let language = gtk::DropDown::from_strings(&["All languages"]);
    language.add_css_class("oma-dropdown");
    language.set_tooltip_text(Some("Filter by language (l)"));

    let filter_bar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    filter_bar.set_margin_top(12);
    filter_bar.set_margin_bottom(8);
    filter_bar.set_margin_start(12);
    filter_bar.set_margin_end(12);
    filter_bar.append(&search);
    filter_bar.append(&language);

    let list = gtk::ListBox::new();
    list.add_css_class("oma-list");
    list.set_selection_mode(gtk::SelectionMode::Single);
    list.set_activate_on_single_click(true);

    let list_scroll = gtk::ScrolledWindow::builder()
        .child(&list)
        .vexpand(true)
        .hexpand(true)
        .build();
    list_scroll.add_css_class("oma-scroll");

    let spinner = gtk::Spinner::new();
    let progress = gtk::Label::builder()
        .label("Loading catalogue…")
        .xalign(0.0)
        .hexpand(true)
        .wrap(true)
        .build();
    progress.add_css_class("oma-muted");
    let cancel = gtk::Button::with_label("Cancel");
    cancel.add_css_class("destructive-action");
    cancel.set_visible(false);

    let bottom = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    bottom.set_margin_top(8);
    bottom.set_margin_bottom(12);
    bottom.set_margin_start(12);
    bottom.set_margin_end(12);
    bottom.append(&spinner);
    bottom.append(&progress);
    bottom.append(&cancel);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&filter_bar);
    content.append(&list_scroll);
    content.append(&bottom);
    download_window.set_child(Some(&content));

    DownloadWidgets {
        window: download_window,
        search,
        language,
        list,
        spinner,
        progress,
        cancel,
        refresh,
    }
}

fn wire_signals(ui: &Rc<Ui>) {
    wire_header(ui);
    wire_reader(ui);
    wire_study(ui);
    wire_settings(ui);
    wire_download(ui);
    wire_keys(ui);
}

fn wire_header(ui: &Rc<Ui>) {
    let widget = ui.widgets.search_button.clone();
    let ui_handle = ui.clone();
    widget.connect_clicked(move |_| ui_handle.open_search());

    let widget = ui.widgets.bookmarks_button.clone();
    let ui_handle = ui.clone();
    widget.connect_clicked(move |_| ui_handle.show_study());

    let widget = ui.widgets.download_button.clone();
    let ui_handle = ui.clone();
    widget.connect_clicked(move |_| ui_handle.show_download());

    let widget = ui.widgets.settings_button.clone();
    let ui_handle = ui.clone();
    widget.connect_clicked(move |_| ui_handle.show_settings());

    let widget = ui.widgets.help_button.clone();
    let ui_handle = ui.clone();
    widget.connect_clicked(move |_| ui_handle.show_help());
}

fn wire_reader(ui: &Rc<Ui>) {
    let widget = ui.widgets.translation_dropdown.clone();
    let ui_handle = ui.clone();
    widget.connect_selected_notify(move |dropdown| {
        let index = dropdown.selected();
        if index != gtk::INVALID_LIST_POSITION {
            ui_handle.change_translation(index as usize);
        }
    });

    let widget = ui.widgets.books.clone();
    let ui_handle = ui.clone();
    widget.connect_row_selected(move |_, row| {
        if let Some(row) = row {
            ui_handle.change_book(row.index() as usize);
        }
    });

    let widget = ui.widgets.chapters.clone();
    let ui_handle = ui.clone();
    widget.connect_row_selected(move |_, row| {
        if let Some(row) = row {
            ui_handle.change_chapter(row.index() as usize);
        }
    });

    let widget = ui.widgets.verses.clone();
    let ui_handle = ui.clone();
    widget.connect_row_selected(move |_, row| {
        if let Some(row) = row {
            ui_handle.change_verse(row.index() as usize);
        }
    });

    for (list, focus) in [
        (ui.widgets.books.clone(), 0),
        (ui.widgets.chapters.clone(), 1),
        (ui.widgets.verses.clone(), 2),
    ] {
        let controller = gtk::EventControllerFocus::new();
        let ui_handle = ui.clone();
        controller.connect_enter(move |_| match focus {
            0 => ui_handle.focus_books(),
            1 => ui_handle.focus_chapters(),
            _ => ui_handle.focus_verses(),
        });
        list.add_controller(controller);
    }

    let widget = ui.widgets.search_entry.clone();
    let ui_handle = ui.clone();
    widget.connect_search_changed(move |entry| {
        ui_handle.set_search(entry.text().as_str());
    });

    let widget = ui.widgets.search_entry.clone();
    let ui_handle = ui.clone();
    widget.connect_activate(move |_| {
        ui_handle.widgets.books.grab_focus();
    });

    let widget = ui.widgets.search_entry.clone();
    let ui_handle = ui.clone();
    widget.connect_stop_search(move |_| ui_handle.finish_search());
}

fn wire_study(ui: &Rc<Ui>) {
    let widget = ui.widgets.bookmarks_list.clone();
    let ui_handle = ui.clone();
    widget.connect_row_activated(move |_, row| {
        ui_handle.jump_to_bookmark(row.index() as usize);
    });

    let widget = ui.widgets.bookmarks_note.clone();
    let ui_handle = ui.clone();
    widget.connect_clicked(move |_| ui_handle.edit_note_selected());

    let widget = ui.widgets.bookmarks_remove.clone();
    let ui_handle = ui.clone();
    widget.connect_clicked(move |_| ui_handle.delete_selected_bookmark());

    let widget = ui.widgets.bookmarks_clear.clone();
    let ui_handle = ui.clone();
    let parent = ui.widgets.window.clone();
    widget.connect_clicked(move |_| confirm_clear_bookmarks(&ui_handle, &parent));
}

fn wire_settings(ui: &Rc<Ui>) {
    if let Some(dropdown) = ui.widgets.settings.theme_dropdown.clone() {
        let ui_handle = ui.clone();
        dropdown.connect_selected_notify(move |dropdown| {
            let index = dropdown.selected();
            if index != gtk::INVALID_LIST_POSITION {
                ui_handle.apply_theme(index as usize);
            }
        });
    }

    let widget = ui.widgets.settings.verse_switch.clone();
    let ui_handle = ui.clone();
    widget.connect_active_notify(move |switch| {
        ui_handle.set_verse_numbers(switch.is_active());
    });

    let widget = ui.widgets.settings.font_spin.clone();
    let ui_handle = ui.clone();
    widget.connect_value_changed(move |spin| {
        ui_handle.set_font_size(spin.value());
    });

    let widget = ui.widgets.settings.add_button.clone();
    let ui_handle = ui.clone();
    widget.connect_clicked(move |_| ui_handle.show_download());

    let widget = ui.widgets.settings.clear_button.clone();
    let ui_handle = ui.clone();
    let parent = ui.widgets.settings.window.clone();
    widget.connect_clicked(move |_| confirm_clear_bookmarks(&ui_handle, &parent));
}

fn wire_download(ui: &Rc<Ui>) {
    let widget = ui.widgets.download.search.clone();
    let ui_handle = ui.clone();
    widget.connect_search_changed(move |entry| {
        ui_handle.set_download_query(entry.text().as_str());
    });

    let widget = ui.widgets.download.language.clone();
    let ui_handle = ui.clone();
    widget.connect_selected_notify(move |dropdown| {
        let index = dropdown.selected();
        if index != gtk::INVALID_LIST_POSITION {
            ui_handle.set_download_language(index as usize);
        }
    });

    let widget = ui.widgets.download.list.clone();
    let ui_handle = ui.clone();
    widget.connect_row_activated(move |_, row| {
        ui_handle.start_download(row.index() as usize);
    });

    let widget = ui.widgets.download.cancel.clone();
    let ui_handle = ui.clone();
    widget.connect_clicked(move |_| ui_handle.cancel_download());

    let widget = ui.widgets.download.refresh.clone();
    let ui_handle = ui.clone();
    widget.connect_clicked(move |_| ui_handle.refresh_catalog());

    let controller = gtk::EventControllerKey::new();
    controller.set_propagation_phase(gtk::PropagationPhase::Capture);
    let ui_handle = ui.clone();
    controller.connect_key_pressed(move |_, key, _, modifiers| {
        ui_handle.handle_download_key(key, modifiers)
    });
    ui.widgets.download.window.add_controller(controller);
}

fn wire_keys(ui: &Rc<Ui>) {
    let controller = gtk::EventControllerKey::new();
    controller.set_propagation_phase(gtk::PropagationPhase::Capture);
    let ui_handle = ui.clone();
    controller
        .connect_key_pressed(move |_, key, _, modifiers| ui_handle.handle_key(key, modifiers));
    ui.widgets.window.add_controller(controller);

    let ui_handle = ui.clone();
    ui.widgets.window.connect_close_request(move |_| {
        ui_handle.save_position();
        glib::Propagation::Proceed
    });

    let ui_handle = ui.clone();
    glib::timeout_add_local(Duration::from_millis(120), move || {
        ui_handle.poll_worker();
        ui_handle.update_download_progress();
        glib::ControlFlow::Continue
    });

    let ui_handle = ui.clone();
    glib::timeout_add_seconds_local(1, move || {
        ui_handle.refresh_theme();
        glib::ControlFlow::Continue
    });
}

fn confirm_clear_bookmarks(ui: &Rc<Ui>, parent: &impl IsA<gtk::Window>) {
    let dialog = adw::MessageDialog::new(
        Some(parent),
        Some("Clear all bookmarks?"),
        Some("Every saved verse and note will be removed. This cannot be undone."),
    );
    dialog.add_response("cancel", "Cancel");
    dialog.add_response("clear", "Clear");
    dialog.set_response_appearance("clear", adw::ResponseAppearance::Destructive);
    dialog.set_default_response(Some("cancel"));
    dialog.set_close_response("cancel");

    let ui_handle = ui.clone();
    dialog.connect_response(None, move |dialog: &adw::MessageDialog, response: &str| {
        if response == "clear" {
            ui_handle.clear_bookmarks();
        }
        dialog.close();
    });

    dialog.present();
}
