use std::{
    cell::RefCell,
    rc::Rc,
    sync::mpsc::{Receiver, Sender, TryRecvError},
    thread,
};

use adw::prelude::*;
use gtk::{gdk, glib};
use oma_bible_core::{
    bible::Library,
    remote::{DownloadControl, RemoteCatalog, download_translation, save_download},
    storage::{Bookmark, Bookmarks, Settings},
    theme::ThemeCatalog,
};

use crate::widgets::Widgets;

pub(crate) enum WorkerMessage {
    Catalog(Result<RemoteCatalog, String>),
    Download(Result<DownloadDone, String>),
}

pub(crate) struct DownloadDone {
    pub id: String,
    pub name: String,
}

pub(crate) struct Initial {
    pub library: Library,
    pub settings: Settings,
    pub bookmarks: Bookmarks,
    pub themes: ThemeCatalog,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Focus {
    Books,
    Chapters,
    Verses,
}

impl Focus {
    fn next(self) -> Self {
        match self {
            Self::Books => Self::Chapters,
            Self::Chapters => Self::Verses,
            Self::Verses => Self::Books,
        }
    }

    fn previous(self) -> Self {
        match self {
            Self::Books => Self::Verses,
            Self::Chapters => Self::Books,
            Self::Verses => Self::Chapters,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Books => "Books",
            Self::Chapters => "Chapters",
            Self::Verses => "Verses",
        }
    }
}

#[derive(Clone)]
pub(crate) struct NoteTarget {
    pub translation: String,
    pub book: String,
    pub chapter: u32,
    pub verse: u32,
}

pub(crate) struct DownloadState {
    pub id: String,
    pub name: String,
    pub control: DownloadControl,
}

struct State {
    library: Library,
    settings: Settings,
    bookmarks: Bookmarks,
    themes: ThemeCatalog,

    translation: usize,
    book: usize,
    chapter: usize,
    verse: usize,
    focus: Focus,

    search: String,
    search_matches: Vec<usize>,
    search_index: usize,
    pending_g: bool,
    syncing: bool,

    download: Option<DownloadState>,
    catalog: Option<RemoteCatalog>,
    catalog_loading: bool,
    catalog_error: Option<String>,
    download_language: Option<String>,
    download_query: String,

    worker_tx: Sender<WorkerMessage>,
    worker_rx: Receiver<WorkerMessage>,
}

impl State {
    fn new(
        initial: Initial,
        worker_tx: Sender<WorkerMessage>,
        worker_rx: Receiver<WorkerMessage>,
    ) -> Self {
        let Initial {
            library,
            settings,
            bookmarks,
            themes,
        } = initial;
        let download_language = settings.download_language.clone();

        let mut state = Self {
            library,
            settings,
            bookmarks,
            themes,
            translation: 0,
            book: 0,
            chapter: 0,
            verse: 0,
            focus: Focus::Books,
            search: String::new(),
            search_matches: Vec::new(),
            search_index: 0,
            pending_g: false,
            syncing: false,
            download: None,
            catalog: None,
            catalog_loading: false,
            catalog_error: None,
            download_language,
            download_query: String::new(),
            worker_tx,
            worker_rx,
        };

        state.restore_position();
        state
    }

    fn restore_position(&mut self) {
        if let Some(abbreviation) = self.settings.translation.clone()
            && let Some(index) = self.library.translations().iter().position(|translation| {
                translation.abbreviation.eq_ignore_ascii_case(&abbreviation)
            })
        {
            self.translation = index;
        }

        if let Some(name) = self.settings.book.clone() {
            self.book = self
                .translation()
                .and_then(|translation| {
                    translation
                        .books
                        .iter()
                        .position(|book| book.name.eq_ignore_ascii_case(&name))
                })
                .unwrap_or(0);
        }

        if let Some(number) = self.settings.chapter {
            self.chapter = self
                .book()
                .and_then(|book| {
                    book.chapters
                        .iter()
                        .position(|chapter| chapter.number == number)
                })
                .unwrap_or(0);
        }

        if let Some(number) = self.settings.verse
            && let Some(chapter) = self.chapter()
        {
            self.verse = (number as usize)
                .saturating_sub(1)
                .min(chapter.verse_count().saturating_sub(1));
        }
    }

    fn translation(&self) -> Option<&oma_bible_core::bible::Translation> {
        self.library.translations().get(self.translation)
    }

    fn book(&self) -> Option<&oma_bible_core::bible::BibleBook> {
        self.translation()?.books.get(self.book)
    }

    fn chapter(&self) -> Option<&oma_bible_core::bible::Chapter> {
        self.book()?.chapters.get(self.chapter)
    }

    fn current_reference(&self) -> Option<NoteTarget> {
        let translation = self.translation()?;
        let book = self.book()?;
        let chapter = self.chapter()?;

        Some(NoteTarget {
            translation: translation.abbreviation.clone(),
            book: book.name.clone(),
            chapter: chapter.number,
            verse: u32::try_from(self.verse).unwrap_or(u32::MAX - 1) + 1,
        })
    }

    fn current_bookmarked(&self) -> bool {
        self.current_reference().is_some_and(|target| {
            self.bookmarks
                .find(
                    &target.translation,
                    &target.book,
                    target.chapter,
                    target.verse,
                )
                .is_some()
        })
    }

    fn clamp_indices(&mut self) {
        let translations = self.library.translations().len();
        if translations == 0 {
            self.translation = 0;
            self.book = 0;
            self.chapter = 0;
            self.verse = 0;
            return;
        }

        self.translation = self.translation.min(translations - 1);
        let books = self.translation().map(|item| item.books.len()).unwrap_or(0);
        if books == 0 {
            self.book = 0;
            self.chapter = 0;
            self.verse = 0;
            return;
        }

        self.book = self.book.min(books - 1);
        let chapters = self.book().map(|item| item.chapters.len()).unwrap_or(0);
        if chapters == 0 {
            self.chapter = 0;
            self.verse = 0;
            return;
        }

        self.chapter = self.chapter.min(chapters - 1);
        let verses = self.chapter().map(|item| item.verse_count()).unwrap_or(0);
        self.verse = self.verse.min(verses.saturating_sub(1));
    }
}

pub(crate) struct Ui {
    state: RefCell<State>,
    pub(crate) widgets: Widgets,
}

impl Ui {
    pub(crate) fn new(
        initial: Initial,
        worker_tx: Sender<WorkerMessage>,
        worker_rx: Receiver<WorkerMessage>,
        widgets: Widgets,
    ) -> Rc<Self> {
        Rc::new(Self {
            state: RefCell::new(State::new(initial, worker_tx, worker_rx)),
            widgets,
        })
    }

    pub(crate) fn initial_load(self: &Rc<Self>) {
        self.refresh_translation_dropdown();
        self.refresh_all();
        self.refresh_bookmarks_panel();
        self.update_theme_css();
        self.update_settings_view();
    }

    pub(crate) fn has_translations(&self) -> bool {
        !self.state.borrow().library.translations().is_empty()
    }

    pub(crate) fn needs_theme_picker(&self) -> bool {
        let state = self.state.borrow();
        !state.themes.is_on_omarchy()
            && state.settings.theme.is_none()
            && state.themes.themes().len() > 1
    }

    fn translation_names(&self) -> (Vec<String>, usize) {
        let state = self.state.borrow();
        (
            state
                .library
                .translations()
                .iter()
                .map(|translation| format!("{} ({})", translation.name, translation.abbreviation))
                .collect(),
            state.translation,
        )
    }

    fn refresh_translation_dropdown(&self) {
        let (names, selected) = self.translation_names();
        let names = names.iter().map(String::as_str).collect::<Vec<_>>();
        let model = gtk::StringList::new(&names);

        {
            let mut state = self.state.borrow_mut();
            state.syncing = true;
        }
        self.widgets.translation_dropdown.set_model(Some(&model));
        self.widgets
            .translation_dropdown
            .set_selected(selected.min(names.len().saturating_sub(1)) as u32);
        {
            let mut state = self.state.borrow_mut();
            state.syncing = false;
        }
    }

    fn refresh_all(&self) {
        {
            let mut state = self.state.borrow_mut();
            state.syncing = true;
            state.clamp_indices();
        }

        self.show_books();
        self.show_chapters();
        self.show_verses();

        {
            let mut state = self.state.borrow_mut();
            state.syncing = false;
        }
        self.update_footer();
    }

    fn show_books(&self) {
        let (names, selected) = {
            let state = self.state.borrow();
            (
                state
                    .translation()
                    .map(|translation| {
                        translation
                            .books
                            .iter()
                            .map(|book| book.name.clone())
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default(),
                state.book,
            )
        };

        self.widgets.books.remove_all();
        for name in &names {
            add_row(&self.widgets.books, name);
        }
        select_row(&self.widgets.books, selected);
    }

    fn show_chapters(&self) {
        let (numbers, selected) = {
            let state = self.state.borrow();
            (
                state
                    .book()
                    .map(|book| {
                        book.chapters
                            .iter()
                            .map(|chapter| chapter.number.to_string())
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default(),
                state.chapter,
            )
        };

        self.widgets.chapters.remove_all();
        for number in &numbers {
            add_row(&self.widgets.chapters, number);
        }
        select_row(&self.widgets.chapters, selected);
    }

    fn show_verses(&self) {
        let (verses, selected, numbers, query, bookmarked, highlight, heading) = {
            let state = self.state.borrow();
            let Some(chapter) = state.chapter() else {
                self.widgets.verses.remove_all();
                self.widgets.heading.set_text("");
                return;
            };
            let Some(book) = state.book() else {
                self.widgets.verses.remove_all();
                self.widgets.heading.set_text("");
                return;
            };
            let Some(translation) = state.translation() else {
                self.widgets.verses.remove_all();
                self.widgets.heading.set_text("");
                return;
            };
            let bookmarked = state
                .bookmarks
                .items()
                .iter()
                .filter(|bookmark| {
                    bookmark
                        .translation
                        .eq_ignore_ascii_case(&translation.abbreviation)
                        && bookmark.book.eq_ignore_ascii_case(&book.name)
                        && bookmark.chapter == chapter.number
                })
                .map(|bookmark| bookmark.verse as usize)
                .collect::<Vec<_>>();
            (
                chapter.verses.clone(),
                state.verse,
                state.settings.show_verse_numbers,
                state.search.clone(),
                bookmarked,
                state.themes.active().selection.to_hex(),
                format!("{} {}", book.name, chapter.number),
            )
        };

        self.widgets.heading.set_text(&heading);
        self.widgets.verses.remove_all();
        for (index, verse) in verses.iter().enumerate() {
            let number = index + 1;
            let number_label = gtk::Label::builder()
                .label(number.to_string())
                .xalign(1.0)
                .yalign(0.0)
                .width_chars(3)
                .visible(numbers)
                .build();
            number_label.add_css_class("oma-verse-number");
            if bookmarked.contains(&number) {
                number_label.add_css_class("oma-verse-number-bookmarked");
            }

            let text_label = gtk::Label::builder()
                .xalign(0.0)
                .wrap(true)
                .selectable(true)
                .hexpand(true)
                .build();
            text_label.add_css_class("oma-verse-text");
            if query.trim().is_empty() {
                text_label.set_text(verse);
            } else {
                text_label.set_markup(&highlight_markup(verse, &query, &highlight));
            }

            let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
            row_box.append(&number_label);
            row_box.append(&text_label);

            let row = gtk::ListBoxRow::builder()
                .child(&row_box)
                .activatable(false)
                .build();
            row.add_css_class("oma-verse-row");
            self.widgets.verses.append(&row);
        }

        select_row(&self.widgets.verses, selected);
        self.scroll_to_verse(selected);
    }

    fn scroll_to_verse(&self, index: usize) {
        let count = {
            let state = self.state.borrow();
            state
                .chapter()
                .map(|chapter| chapter.verse_count())
                .unwrap_or(0)
        };
        if count == 0 {
            return;
        }

        let adjustment = self.widgets.reading_scroll.vadjustment();
        let span = (adjustment.upper() - adjustment.page_size()).max(0.0);
        let fraction = index as f64 / count as f64;
        adjustment.set_value(span * fraction.min(1.0));
    }

    fn update_footer(&self) {
        let text = {
            let state = self.state.borrow();
            let reference = match (state.translation(), state.book(), state.chapter()) {
                (Some(translation), Some(book), Some(chapter)) => format!(
                    "{} · {} {}:{}{}",
                    translation.abbreviation,
                    book.name,
                    chapter.number,
                    state.verse + 1,
                    if state.current_bookmarked() {
                        " · bookmarked"
                    } else {
                        ""
                    }
                ),
                _ => String::from("No translations installed"),
            };
            let search = if state.search.trim().is_empty() {
                String::new()
            } else {
                format!(
                    "  ·  /{} ({}/{})",
                    state.search,
                    state.search_index + usize::from(!state.search_matches.is_empty()),
                    state.search_matches.len()
                )
            };
            format!("{}  ·  {reference}{search}", state.focus.label())
        };

        self.widgets.status.set_text(&text);
    }

    pub(crate) fn save_position(&self) {
        let (translation, book, chapter, verse) = {
            let state = self.state.borrow();
            (
                state.translation().map(|item| item.abbreviation.clone()),
                state.book().map(|item| item.name.clone()),
                state.chapter().map(|item| item.number),
                (state.chapter().is_some()).then(|| {
                    u32::try_from(state.verse)
                        .unwrap_or(u32::MAX - 1)
                        .saturating_add(1)
                }),
            )
        };

        let mut state = self.state.borrow_mut();
        state.settings.translation = translation;
        state.settings.book = book;
        state.settings.chapter = chapter;
        state.settings.verse = verse;
        state.settings.download_language = state.download_language.clone();
        if !state.themes.is_on_omarchy() {
            state.settings.theme = Some(state.themes.active().slug.clone());
        }
        let error = state.settings.save().err();
        drop(state);

        if let Some(error) = error {
            self.widgets
                .status
                .set_text(&format!("Could not save settings: {error}"));
        }
    }

    pub(crate) fn change_translation(&self, index: usize) {
        {
            let mut state = self.state.borrow_mut();
            if state.syncing || state.library.translations().is_empty() {
                return;
            }
            state.translation = index.min(state.library.translations().len() - 1);
            state.book = 0;
            state.chapter = 0;
            state.verse = 0;
            state.search_matches.clear();
            state.search_index = 0;
        }

        self.refresh_all();
        self.save_position();
    }

    pub(crate) fn change_book(&self, index: usize) {
        {
            let mut state = self.state.borrow_mut();
            if state.syncing || state.book == index {
                return;
            }
            state.book = index;
            state.chapter = 0;
            state.verse = 0;
            state.search_index = 0;
        }

        self.recompute_search();
        self.show_chapters();
        self.show_verses();
        self.save_position();
        self.update_footer();
    }

    pub(crate) fn change_chapter(&self, index: usize) {
        {
            let mut state = self.state.borrow_mut();
            if state.syncing || state.chapter == index {
                return;
            }
            state.chapter = index;
            state.verse = 0;
            state.search_index = 0;
        }

        self.recompute_search();
        self.show_verses();
        self.save_position();
        self.update_footer();
    }

    pub(crate) fn change_verse(&self, index: usize) {
        let mut state = self.state.borrow_mut();
        if state.syncing {
            return;
        }
        let count = state
            .chapter()
            .map(|chapter| chapter.verse_count())
            .unwrap_or(0);
        if count == 0 {
            return;
        }
        state.verse = index.min(count - 1);
        drop(state);
        self.update_footer();
    }

    fn select_verse(&self, index: usize) {
        let selected = {
            let mut state = self.state.borrow_mut();
            let count = state
                .chapter()
                .map(|chapter| chapter.verse_count())
                .unwrap_or(0);
            if count == 0 {
                return;
            }
            let selected = index.min(count - 1);
            state.verse = selected;
            state.syncing = true;
            selected
        };

        select_row(&self.widgets.verses, selected);
        self.scroll_to_verse(selected);
        {
            let mut state = self.state.borrow_mut();
            state.syncing = false;
        }
        self.update_footer();
    }

    fn move_selection(&self, delta: isize) {
        match self.state.borrow().focus {
            Focus::Books => {
                let (index, count) = {
                    let state = self.state.borrow();
                    (
                        state.book,
                        state
                            .translation()
                            .map(|translation| translation.books.len())
                            .unwrap_or(0),
                    )
                };
                select_row(&self.widgets.books, step(index, delta, count));
            }
            Focus::Chapters => {
                let (index, count) = {
                    let state = self.state.borrow();
                    (
                        state.chapter,
                        state.book().map(|book| book.chapters.len()).unwrap_or(0),
                    )
                };
                select_row(&self.widgets.chapters, step(index, delta, count));
            }
            Focus::Verses => {
                let (index, count) = {
                    let state = self.state.borrow();
                    (
                        state.verse,
                        state
                            .chapter()
                            .map(|chapter| chapter.verse_count())
                            .unwrap_or(0),
                    )
                };
                self.select_verse(step(index, delta, count));
            }
        }
    }

    pub(crate) fn focus_books(&self) {
        self.set_focus(Focus::Books);
    }

    pub(crate) fn focus_chapters(&self) {
        self.set_focus(Focus::Chapters);
    }

    pub(crate) fn focus_verses(&self) {
        self.set_focus(Focus::Verses);
    }

    fn set_focus(&self, focus: Focus) {
        {
            let mut state = self.state.borrow_mut();
            if state.syncing || state.focus == focus {
                return;
            }
            state.focus = focus;
        }
        self.update_footer();
    }

    fn focus_next(&self) {
        let mut state = self.state.borrow_mut();
        state.focus = state.focus.next();
        drop(state);
        self.update_footer();
    }

    fn focus_previous(&self) {
        let mut state = self.state.borrow_mut();
        state.focus = state.focus.previous();
        drop(state);
        self.update_footer();
    }

    fn go_top(&self) {
        match self.state.borrow().focus {
            Focus::Books => select_row(&self.widgets.books, 0),
            Focus::Chapters => select_row(&self.widgets.chapters, 0),
            Focus::Verses => self.select_verse(0),
        }
    }

    fn go_bottom(&self) {
        match self.state.borrow().focus {
            Focus::Books => {
                let count = self
                    .state
                    .borrow()
                    .translation()
                    .map(|translation| translation.books.len())
                    .unwrap_or(0);
                select_row(&self.widgets.books, count.saturating_sub(1));
            }
            Focus::Chapters => {
                let count = self
                    .state
                    .borrow()
                    .book()
                    .map(|book| book.chapters.len())
                    .unwrap_or(0);
                select_row(&self.widgets.chapters, count.saturating_sub(1));
            }
            Focus::Verses => {
                let count = self
                    .state
                    .borrow()
                    .chapter()
                    .map(|chapter| chapter.verse_count())
                    .unwrap_or(0);
                self.select_verse(count.saturating_sub(1));
            }
        }
    }

    fn page(&self, direction: isize) {
        match self.state.borrow().focus {
            Focus::Verses => self.move_selection(8 * direction),
            _ => self.move_selection(5 * direction),
        }
    }

    fn next_chapter(&self) {
        let target = {
            let state = self.state.borrow();
            let Some(translation) = state.translation() else {
                return;
            };
            if state.chapter + 1 < state.book().map(|book| book.chapters.len()).unwrap_or(0) {
                Some((state.translation, state.book, state.chapter + 1, 0))
            } else {
                (state.book + 1..translation.books.len())
                    .find(|&book| !translation.books[book].chapters.is_empty())
                    .map(|book| (state.translation, book, 0, 0))
            }
        };

        if let Some((translation, book, chapter, verse)) = target {
            self.jump_to_position(translation, book, chapter, verse);
        }
    }

    fn previous_chapter(&self) {
        let target = {
            let state = self.state.borrow();
            let Some(translation) = state.translation() else {
                return;
            };
            if state.chapter > 0 {
                Some((state.translation, state.book, state.chapter - 1, 0))
            } else {
                (0..state.book)
                    .rev()
                    .find(|&book| !translation.books[book].chapters.is_empty())
                    .map(|book| {
                        (
                            state.translation,
                            book,
                            translation.books[book].chapters.len() - 1,
                            0,
                        )
                    })
            }
        };

        if let Some((translation, book, chapter, verse)) = target {
            self.jump_to_position(translation, book, chapter, verse);
        }
    }

    fn jump_to_position(&self, translation: usize, book: usize, chapter: usize, verse: usize) {
        {
            let mut state = self.state.borrow_mut();
            state.translation = translation;
            state.book = book;
            state.chapter = chapter;
            state.verse = verse;
            state.clamp_indices();
        }

        self.refresh_translation_dropdown();
        self.refresh_all();
        self.save_position();
    }

    fn recompute_search(&self) {
        let first = {
            let mut state = self.state.borrow_mut();
            let query = state.search.trim().to_lowercase();
            state.search_matches = if query.is_empty() {
                Vec::new()
            } else {
                state
                    .chapter()
                    .map(|chapter| {
                        chapter
                            .verses
                            .iter()
                            .enumerate()
                            .filter(|(_, verse)| verse.to_lowercase().contains(&query))
                            .map(|(index, _)| index)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default()
            };
            state.search_index = 0;
            state.search_matches.first().copied()
        };

        if let Some(index) = first {
            let mut state = self.state.borrow_mut();
            state.verse = index;
        }
    }

    pub(crate) fn set_search(&self, query: &str) {
        {
            let mut state = self.state.borrow_mut();
            if state.syncing {
                return;
            }
            state.search = query.trim().to_string();
        }

        self.recompute_search();
        self.show_verses();
        self.update_footer();
    }

    pub(crate) fn search_next(&self, forward: bool) {
        let target = {
            let mut state = self.state.borrow_mut();
            if state.search_matches.is_empty() {
                return;
            }
            let count = state.search_matches.len();
            state.search_index = if forward {
                (state.search_index + 1) % count
            } else {
                (state.search_index + count - 1) % count
            };
            state.search_matches[state.search_index]
        };

        self.select_verse(target);
    }

    pub(crate) fn open_search(&self) {
        self.widgets.search_entry.set_visible(true);
        self.widgets.search_entry.grab_focus();
        self.widgets.search_entry.select_region(0, -1);
    }

    pub(crate) fn finish_search(self: &Rc<Self>) {
        {
            let mut state = self.state.borrow_mut();
            state.search.clear();
            state.search_matches.clear();
            state.search_index = 0;
            state.syncing = true;
        }
        self.widgets.search_entry.set_text("");
        self.widgets.search_entry.set_visible(false);
        {
            let mut state = self.state.borrow_mut();
            state.syncing = false;
        }
        self.show_verses();
        self.update_footer();
        self.widgets.verses.grab_focus();
    }

    fn update_theme_css(&self) {
        let (theme, font_size) = {
            let state = self.state.borrow();
            (state.themes.active().clone(), state.settings.font_size)
        };
        self.widgets
            .provider
            .load_from_string(&crate::widgets::theme_css(&theme, font_size));
    }

    /// Follow an Omarchy theme change while the reader is running.
    pub(crate) fn refresh_theme(&self) {
        let changed = {
            let mut state = self.state.borrow_mut();
            state.themes.refresh_active()
        };
        if !changed {
            return;
        }

        let search_active = {
            let state = self.state.borrow();
            !state.search.trim().is_empty()
        };
        self.update_theme_css();
        if search_active {
            // Search highlighting is rendered with inline colours, so rebuild
            // those rows; everything else follows the CSS provider.
            self.show_verses();
        }
    }

    pub(crate) fn apply_theme(&self, index: usize) {
        let (changed, slug, on_omarchy) = {
            let mut state = self.state.borrow_mut();
            if state.syncing {
                return;
            }
            let changed = state.themes.select(index);
            (
                changed,
                state.themes.active().slug.clone(),
                state.themes.is_on_omarchy(),
            )
        };

        if !changed {
            return;
        }

        {
            let mut state = self.state.borrow_mut();
            if !on_omarchy {
                state.settings.theme = Some(slug);
            }
            let _ = state.settings.save();
        }

        self.update_theme_css();
        self.update_footer();
    }

    pub(crate) fn set_verse_numbers(&self, show: bool) {
        {
            let mut state = self.state.borrow_mut();
            if state.syncing {
                return;
            }
            state.settings.show_verse_numbers = show;
            let _ = state.settings.save();
        }
        self.show_verses();
    }

    pub(crate) fn set_font_size(&self, size: f64) {
        {
            let mut state = self.state.borrow_mut();
            if state.syncing {
                return;
            }
            state.settings.font_size = (size.round() as u32).clamp(12, 30);
            let _ = state.settings.save();
        }
        self.update_theme_css();
    }

    pub(crate) fn update_settings_view(&self) {
        let (show_numbers, font_size, bookmarks, theme_index) = {
            let state = self.state.borrow();
            (
                state.settings.show_verse_numbers,
                state.settings.font_size,
                state.bookmarks.len(),
                state.themes.current_index().unwrap_or(0),
            )
        };

        {
            let mut state = self.state.borrow_mut();
            state.syncing = true;
        }
        self.widgets.settings.verse_switch.set_active(show_numbers);
        self.widgets.settings.font_spin.set_value(font_size as f64);
        self.widgets
            .settings
            .bookmarks_row
            .set_subtitle(&format!("{bookmarks} saved"));
        if let Some(dropdown) = &self.widgets.settings.theme_dropdown {
            dropdown.set_selected(theme_index as u32);
        }
        {
            let mut state = self.state.borrow_mut();
            state.syncing = false;
        }
    }

    pub(crate) fn show_settings(&self) {
        self.update_settings_view();
        self.widgets.settings.window.present();
    }

    pub(crate) fn show_study(&self) {
        self.refresh_bookmarks_panel();
        self.widgets.split.set_show_sidebar(true);
        self.widgets.bookmarks_list.grab_focus();
    }

    pub(crate) fn hide_study(&self) {
        self.widgets.split.set_show_sidebar(false);
    }

    pub(crate) fn show_download(&self) {
        self.widgets.download.window.present();
        if self.state.borrow().catalog.is_none() && !self.state.borrow().catalog_loading {
            self.refresh_catalog();
        } else {
            self.refresh_download_languages();
            self.refresh_download_list();
        }
        self.update_download_progress();
    }

    pub(crate) fn refresh_bookmarks_panel(&self) {
        let (items, count) = {
            let state = self.state.borrow();
            (
                state
                    .bookmarks
                    .items()
                    .iter()
                    .rev()
                    .cloned()
                    .collect::<Vec<_>>(),
                state.bookmarks.len(),
            )
        };

        self.widgets.bookmarks_list.remove_all();
        for bookmark in &items {
            let reference = gtk::Label::builder()
                .label(bookmark.reference())
                .xalign(0.0)
                .build();
            reference.add_css_class("oma-bookmark-reference");

            let detail = if bookmark.note.is_empty() {
                bookmark.translation.clone()
            } else {
                format!("{} · {}", bookmark.translation, bookmark.note)
            };
            let subtitle = gtk::Label::builder()
                .label(detail)
                .xalign(0.0)
                .wrap(true)
                .build();
            subtitle.add_css_class("oma-muted");

            let content = gtk::Box::new(gtk::Orientation::Vertical, 2);
            content.add_css_class("oma-bookmark-content");
            content.append(&reference);
            content.append(&subtitle);

            let row = gtk::ListBoxRow::builder().child(&content).build();
            row.add_css_class("oma-bookmark-row");
            self.widgets.bookmarks_list.append(&row);
        }

        self.widgets
            .bookmarks_title
            .set_text(&format!("BOOKMARKS ({count})"));
        self.widgets
            .settings
            .bookmarks_row
            .set_subtitle(&format!("{count} saved"));
    }

    pub(crate) fn jump_to_bookmark(&self, index: usize) {
        let bookmark = {
            let state = self.state.borrow();
            state.bookmarks.items().iter().rev().nth(index).cloned()
        };
        let Some(bookmark) = bookmark else {
            return;
        };

        let Some((translation, book, chapter, verse)) = self.locate(&bookmark) else {
            self.widgets
                .status
                .set_text(&format!("{} is no longer available", bookmark.reference()));
            return;
        };

        self.hide_study();
        self.jump_to_position(translation, book, chapter, verse);
        self.select_verse(verse);
        self.widgets.verses.grab_focus();
        self.widgets
            .status
            .set_text(&format!("Jumped to {}", bookmark.reference()));
    }

    pub(crate) fn study_move(&self, delta: isize) {
        let count = self.state.borrow().bookmarks.len();
        if count == 0 {
            return;
        }

        let current = self
            .widgets
            .bookmarks_list
            .selected_row()
            .map(|row| row.index() as usize)
            .unwrap_or_else(|| if delta > 0 { 0 } else { count - 1 });
        select_row(&self.widgets.bookmarks_list, step(current, delta, count));
    }

    pub(crate) fn study_activate(&self) {
        let Some(row) = self.widgets.bookmarks_list.selected_row() else {
            return;
        };
        self.jump_to_bookmark(row.index() as usize);
    }

    fn locate(&self, bookmark: &Bookmark) -> Option<(usize, usize, usize, usize)> {
        let state = self.state.borrow();
        let translation = state
            .library
            .translations()
            .iter()
            .position(|translation| {
                translation
                    .abbreviation
                    .eq_ignore_ascii_case(&bookmark.translation)
            })?;
        let book = state.library.translations()[translation]
            .books
            .iter()
            .position(|book| book.name.eq_ignore_ascii_case(&bookmark.book))?;
        let chapter = state.library.translations()[translation].books[book]
            .chapters
            .iter()
            .position(|chapter| chapter.number == bookmark.chapter)?;
        let verse_count =
            state.library.translations()[translation].books[book].chapters[chapter].verse_count();
        if verse_count == 0 {
            return None;
        }

        Some((
            translation,
            book,
            chapter,
            (bookmark.verse as usize)
                .saturating_sub(1)
                .min(verse_count - 1),
        ))
    }

    pub(crate) fn toggle_bookmark(&self) {
        let Some(target) = self.state.borrow().current_reference() else {
            return;
        };
        let added = {
            let mut state = self.state.borrow_mut();
            let added = state.bookmarks.toggle(
                &target.translation,
                &target.book,
                target.chapter,
                target.verse,
            );
            let _ = state.bookmarks.save();
            added
        };

        self.refresh_bookmarks_panel();
        self.show_verses();
        self.update_footer();
        self.widgets.status.set_text(&format!(
            "{} {}:{}{}",
            if added { "Bookmarked" } else { "Removed" },
            target.book,
            target.chapter,
            target.verse
        ));
    }

    pub(crate) fn delete_selected_bookmark(&self) {
        let Some(row) = self.widgets.bookmarks_list.selected_row() else {
            return;
        };
        let selected = row.index() as usize;
        let removed = {
            let mut state = self.state.borrow_mut();
            let count = state.bookmarks.len();
            if count == 0 {
                return;
            }
            let actual = count - 1 - selected.min(count - 1);
            let removed = state.bookmarks.remove_index(actual);
            let _ = state.bookmarks.save();
            removed
        };

        if let Some(removed) = removed {
            self.refresh_bookmarks_panel();
            self.show_verses();
            self.update_footer();
            self.widgets
                .status
                .set_text(&format!("Removed {}", removed.reference()));
        }
    }

    pub(crate) fn clear_bookmarks(&self) {
        {
            let mut state = self.state.borrow_mut();
            state.bookmarks.clear();
            let _ = state.bookmarks.save();
        }
        self.refresh_bookmarks_panel();
        self.show_verses();
        self.update_footer();
        self.widgets.status.set_text("Bookmarks cleared");
    }

    pub(crate) fn edit_note_current(self: &Rc<Self>) {
        let Some(target) = self.state.borrow().current_reference() else {
            return;
        };
        let note = self
            .state
            .borrow()
            .bookmarks
            .find(
                &target.translation,
                &target.book,
                target.chapter,
                target.verse,
            )
            .map(|bookmark| bookmark.note.clone())
            .unwrap_or_default();
        self.open_note_dialog(target, note);
    }

    pub(crate) fn edit_note_selected(self: &Rc<Self>) {
        let bookmark = {
            let Some(row) = self.widgets.bookmarks_list.selected_row() else {
                return;
            };
            let selected = row.index() as usize;
            let state = self.state.borrow();
            state.bookmarks.items().iter().rev().nth(selected).cloned()
        };

        if let Some(bookmark) = bookmark {
            self.open_note_dialog(
                NoteTarget {
                    translation: bookmark.translation.clone(),
                    book: bookmark.book.clone(),
                    chapter: bookmark.chapter,
                    verse: bookmark.verse,
                },
                bookmark.note,
            );
        }
    }

    fn open_note_dialog(self: &Rc<Self>, target: NoteTarget, note: String) {
        let dialog = gtk::Window::builder()
            .title("Note")
            .transient_for(&self.widgets.window)
            .modal(true)
            .default_width(460)
            .default_height(170)
            .build();
        dialog.add_css_class("oma-window");

        let header = gtk::HeaderBar::new();
        let save = gtk::Button::with_label("Save");
        save.add_css_class("suggested-action");
        let cancel = gtk::Button::with_label("Cancel");
        header.pack_end(&save);
        header.pack_start(&cancel);
        dialog.set_titlebar(Some(&header));

        let label = gtk::Label::builder()
            .label(format!(
                "Note for {} {}:{}",
                target.book, target.chapter, target.verse
            ))
            .xalign(0.0)
            .build();
        label.add_css_class("oma-muted");

        let entry = gtk::Entry::builder()
            .text(note)
            .placeholder_text("Write a note…")
            .build();
        entry.add_css_class("oma-note-entry");

        let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
        content.set_margin_top(16);
        content.set_margin_bottom(16);
        content.set_margin_start(16);
        content.set_margin_end(16);
        content.append(&label);
        content.append(&entry);
        dialog.set_child(Some(&content));

        {
            let ui = self.clone();
            let dialog = dialog.clone();
            let entry = entry.clone();
            let target = target.clone();
            save.connect_clicked(move |_| {
                ui.save_note(&target, entry.text().as_str());
                dialog.close();
            });
        }

        {
            let ui = self.clone();
            let dialog = dialog.clone();
            let entry_handle = entry.clone();
            let target = target.clone();
            entry.connect_activate(move |_| {
                ui.save_note(&target, entry_handle.text().as_str());
                dialog.close();
            });
        }

        {
            let dialog = dialog.clone();
            cancel.connect_clicked(move |_| dialog.close());
        }

        dialog.present();
        entry.grab_focus();
    }

    pub(crate) fn save_note(&self, target: &NoteTarget, note: &str) {
        {
            let mut state = self.state.borrow_mut();
            state.bookmarks.set_note(
                &target.translation,
                &target.book,
                target.chapter,
                target.verse,
                note,
            );
            let _ = state.bookmarks.save();
        }

        self.refresh_bookmarks_panel();
        self.show_verses();
        self.update_footer();
        self.widgets.status.set_text(&format!(
            "Saved note for {} {}:{}",
            target.book, target.chapter, target.verse
        ));
    }

    pub(crate) fn refresh_catalog(&self) {
        {
            let mut state = self.state.borrow_mut();
            if state.catalog_loading {
                return;
            }
            state.catalog = None;
            state.catalog_loading = true;
            state.catalog_error = None;
        }

        self.widgets
            .download
            .progress
            .set_text("Loading catalogue…");
        self.widgets.download.spinner.start();
        self.widgets.download.cancel.set_visible(false);
        self.refresh_download_list();

        let sender = self.state.borrow().worker_tx.clone();
        thread::spawn(move || {
            let result = RemoteCatalog::fetch().map_err(|error| error.to_string());
            let _ = sender.send(WorkerMessage::Catalog(result));
        });
    }

    fn refresh_download_languages(&self) {
        let (labels, selected) = {
            let state = self.state.borrow();
            match &state.catalog {
                Some(catalog) => {
                    let options = language_options(catalog);
                    let selected = options
                        .iter()
                        .position(|(code, _, _)| *code == state.download_language)
                        .unwrap_or(0);
                    (
                        options
                            .iter()
                            .map(|(_, label, _)| label.clone())
                            .collect::<Vec<_>>(),
                        selected,
                    )
                }
                None => (vec![String::from("All languages")], 0),
            }
        };

        let labels = labels.iter().map(String::as_str).collect::<Vec<_>>();
        let model = gtk::StringList::new(&labels);
        {
            let mut state = self.state.borrow_mut();
            state.syncing = true;
        }
        self.widgets.download.language.set_model(Some(&model));
        self.widgets
            .download
            .language
            .set_selected(selected.min(labels.len().saturating_sub(1)) as u32);
        {
            let mut state = self.state.borrow_mut();
            state.syncing = false;
        }
    }

    pub(crate) fn set_download_language(&self, index: usize) {
        let code = {
            let state = self.state.borrow();
            if state.syncing {
                return;
            }
            let Some(catalog) = &state.catalog else {
                return;
            };
            language_options(catalog)
                .get(index)
                .and_then(|(code, _, _)| code.clone())
        };

        {
            let mut state = self.state.borrow_mut();
            state.download_language = code.clone();
            state.settings.download_language = code;
            let _ = state.settings.save();
        }
        self.refresh_download_list();
    }

    pub(crate) fn set_download_query(&self, query: &str) {
        {
            let mut state = self.state.borrow_mut();
            if state.syncing {
                return;
            }
            state.download_query = query.trim().to_string();
        }
        self.refresh_download_list();
    }

    fn refresh_download_list(&self) {
        let (rows, loading, error) = {
            let state = self.state.borrow();
            let Some(catalog) = &state.catalog else {
                return;
            };
            let installed = state
                .library
                .translations()
                .iter()
                .map(|translation| translation.abbreviation.to_lowercase())
                .collect::<Vec<_>>();
            let rows = catalog
                .filter(state.download_language.as_deref(), &state.download_query)
                .iter()
                .map(|translation| {
                    (
                        translation.id.clone(),
                        translation.title().to_string(),
                        translation.english_title().to_string(),
                        translation.language_label().to_string(),
                        translation.number_of_books,
                        translation.total_number_of_verses,
                        installed.contains(&translation.id.to_lowercase()),
                    )
                })
                .collect::<Vec<_>>();
            (rows, state.catalog_loading, state.catalog_error.clone())
        };

        self.widgets.download.list.remove_all();

        if rows.is_empty() {
            let message = if loading {
                "Loading the catalogue…"
            } else if error.is_some() {
                "Could not load the catalogue. Press refresh to try again."
            } else {
                "No translations match this filter."
            };
            let label = gtk::Label::builder()
                .label(message)
                .xalign(0.0)
                .margin_top(12)
                .margin_bottom(12)
                .margin_start(12)
                .margin_end(12)
                .build();
            label.add_css_class("oma-muted");
            let row = gtk::ListBoxRow::builder()
                .child(&label)
                .activatable(false)
                .selectable(false)
                .build();
            self.widgets.download.list.append(&row);
            return;
        }

        for (id, title, english, language, books, verses, installed) in rows {
            let title_label = gtk::Label::builder().label(&title).xalign(0.0).build();
            title_label.add_css_class("oma-download-title");

            let detail = if installed {
                format!("{id} · {language} · {books} books · {verses} verses · installed")
            } else {
                format!("{id} · {language} · {books} books · {verses} verses")
            };
            let detail_label = gtk::Label::builder()
                .label(detail)
                .xalign(0.0)
                .wrap(true)
                .build();
            detail_label.add_css_class("oma-muted");

            let box_ = gtk::Box::new(gtk::Orientation::Vertical, 2);
            box_.add_css_class("oma-download-content");
            box_.append(&title_label);
            if english != title && !english.trim().is_empty() {
                let english_label = gtk::Label::builder()
                    .label(&english)
                    .xalign(0.0)
                    .wrap(true)
                    .build();
                english_label.add_css_class("oma-muted");
                box_.append(&english_label);
            }
            box_.append(&detail_label);

            let row = gtk::ListBoxRow::builder().child(&box_).build();
            row.add_css_class("oma-download-row");
            self.widgets.download.list.append(&row);
        }
    }

    pub(crate) fn start_download(&self, index: usize) {
        if self.state.borrow().download.is_some() {
            return;
        }

        let item = {
            let state = self.state.borrow();
            let Some(catalog) = &state.catalog else {
                return;
            };
            catalog
                .filter(state.download_language.as_deref(), &state.download_query)
                .get(index)
                .map(|translation| (translation.id.clone(), translation.title().to_string()))
        };
        let Some((id, name)) = item else {
            return;
        };

        let control = DownloadControl::new();
        {
            let mut state = self.state.borrow_mut();
            state.download = Some(DownloadState {
                id: id.clone(),
                name: name.clone(),
                control: control.clone(),
            });
        }

        self.widgets
            .download
            .progress
            .set_text(&format!("Downloading {name}…"));
        self.widgets.download.spinner.start();
        self.widgets.download.cancel.set_visible(true);
        self.widgets.download.cancel.set_sensitive(true);

        let sender = self.state.borrow().worker_tx.clone();
        thread::spawn(move || {
            let result = download_translation(&id, &control)
                .and_then(|translation| {
                    save_download(&translation, &id)?;
                    Ok(DownloadDone {
                        id: id.clone(),
                        name: translation.name,
                    })
                })
                .map_err(|error| error.to_string());
            let _ = sender.send(WorkerMessage::Download(result));
        });
    }

    pub(crate) fn cancel_download(&self) {
        let control = self
            .state
            .borrow()
            .download
            .as_ref()
            .map(|download| download.control.clone());
        if let Some(control) = control {
            control.cancel();
            self.widgets
                .download
                .progress
                .set_text("Cancelling download…");
            self.widgets.download.cancel.set_sensitive(false);
        }
    }

    pub(crate) fn poll_worker(&self) {
        loop {
            let message = match self.state.borrow().worker_rx.try_recv() {
                Ok(message) => message,
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            };

            match message {
                WorkerMessage::Catalog(result) => {
                    let error = {
                        let mut state = self.state.borrow_mut();
                        state.catalog_loading = false;
                        match result {
                            Ok(catalog) => {
                                state.catalog = Some(catalog);
                                state.catalog_error = None;
                                None
                            }
                            Err(error) => {
                                state.catalog_error = Some(error.clone());
                                Some(error)
                            }
                        }
                    };

                    self.widgets.download.spinner.stop();
                    self.refresh_download_languages();
                    self.refresh_download_list();
                    if let Some(error) = error {
                        self.widgets
                            .download
                            .progress
                            .set_text(&format!("Catalogue unavailable: {error}"));
                    } else {
                        self.widgets.download.progress.set_text("");
                    }
                }
                WorkerMessage::Download(result) => {
                    let cancelled = self
                        .state
                        .borrow()
                        .download
                        .as_ref()
                        .is_some_and(|download| download.control.is_cancelled());
                    self.state.borrow_mut().download = None;

                    self.widgets.download.spinner.stop();
                    self.widgets.download.cancel.set_visible(false);

                    match result {
                        Ok(done) => {
                            {
                                let mut state = self.state.borrow_mut();
                                state.library = Library::load_default();
                                if let Some(index) =
                                    state.library.translations().iter().position(|translation| {
                                        translation.abbreviation.eq_ignore_ascii_case(&done.id)
                                    })
                                {
                                    state.translation = index;
                                    state.book = 0;
                                    state.chapter = 0;
                                    state.verse = 0;
                                }
                                state.clamp_indices();
                            }

                            self.refresh_translation_dropdown();
                            self.refresh_all();
                            self.refresh_bookmarks_panel();
                            self.save_position();
                            self.widgets
                                .download
                                .progress
                                .set_text(&format!("Added {} from bible.helloao.org", done.name));
                            self.widgets.status.set_text(&format!(
                                "Added {} · choose another translation or close this window",
                                done.name
                            ));
                            self.refresh_download_list();
                        }
                        Err(error) => {
                            let message = if cancelled {
                                String::from("Download cancelled")
                            } else {
                                format!("Download failed: {error}")
                            };
                            self.widgets.download.progress.set_text(&message);
                            self.widgets.status.set_text(&message);
                        }
                    }
                }
            }
        }
    }

    pub(crate) fn update_download_progress(&self) {
        let progress = {
            let state = self.state.borrow();
            state.download.as_ref().map(|download| {
                (
                    download.name.clone(),
                    download.id.clone(),
                    download.control.downloaded(),
                )
            })
        };

        if let Some((name, id, bytes)) = progress {
            self.widgets.download.spinner.start();
            self.widgets.download.cancel.set_visible(true);
            self.widgets.download.progress.set_text(&format!(
                "Downloading {name} ({id}) · {} · Esc cancels",
                format_bytes(bytes)
            ));
        } else if !self.state.borrow().catalog_loading {
            self.widgets.download.spinner.stop();
            self.widgets.download.cancel.set_visible(false);
        }
    }

    pub(crate) fn handle_key(
        self: &Rc<Self>,
        key: gdk::Key,
        modifiers: gdk::ModifierType,
    ) -> glib::Propagation {
        if self.widgets.search_entry.has_focus() {
            return glib::Propagation::Proceed;
        }

        if self.widgets.translation_dropdown.has_focus() {
            if key.name().as_deref() == Some("Escape") {
                self.widgets.books.grab_focus();
                return glib::Propagation::Stop;
            }
            return glib::Propagation::Proceed;
        }

        let name = key.name().map(|name| name.to_string()).unwrap_or_default();
        let control = modifiers.contains(gdk::ModifierType::CONTROL_MASK);

        if control {
            match name.as_str() {
                "d" => self.page(1),
                "u" => self.page(-1),
                "f" => self.open_search(),
                "q" => self.widgets.window.close(),
                _ => return glib::Propagation::Proceed,
            }
            return glib::Propagation::Stop;
        }

        if self.widgets.split.shows_sidebar() && self.widgets.bookmarks_list.has_focus() {
            match name.as_str() {
                "j" | "Down" => {
                    self.study_move(1);
                    return glib::Propagation::Stop;
                }
                "k" | "Up" => {
                    self.study_move(-1);
                    return glib::Propagation::Stop;
                }
                "Return" | "KP_Enter" => {
                    self.study_activate();
                    return glib::Propagation::Stop;
                }
                "x" | "Delete" => {
                    self.delete_selected_bookmark();
                    return glib::Propagation::Stop;
                }
                "M" => {
                    self.edit_note_selected();
                    return glib::Propagation::Stop;
                }
                "Escape" | "q" | "S" => {
                    self.hide_study();
                    self.widgets.verses.grab_focus();
                    return glib::Propagation::Stop;
                }
                _ => {}
            }
        }

        if name == "g" {
            let repeated = {
                let mut state = self.state.borrow_mut();
                let repeated = state.pending_g;
                state.pending_g = !repeated;
                repeated
            };
            if repeated {
                self.go_top();
            }
            return glib::Propagation::Stop;
        }

        self.state.borrow_mut().pending_g = false;

        let handled = match name.as_str() {
            "Escape" => {
                if self.widgets.split.shows_sidebar() {
                    self.hide_study();
                    self.widgets.verses.grab_focus();
                } else if !self.state.borrow().search.is_empty() {
                    self.finish_search();
                }
                true
            }
            "q" => {
                if self.widgets.split.shows_sidebar() {
                    self.hide_study();
                    self.widgets.verses.grab_focus();
                } else {
                    self.widgets.window.close();
                }
                true
            }
            "j" | "Down" => {
                self.move_selection(1);
                true
            }
            "k" | "Up" => {
                self.move_selection(-1);
                true
            }
            "h" | "Left" => {
                self.focus_previous();
                true
            }
            "l" | "Right" | "Tab" => {
                self.focus_next();
                true
            }
            "G" => {
                self.go_bottom();
                true
            }
            "Home" => {
                self.go_top();
                true
            }
            "End" => {
                self.go_bottom();
                true
            }
            "Page_Down" | "space" => {
                self.page(1);
                true
            }
            "Page_Up" => {
                self.page(-1);
                true
            }
            "bracketleft" => {
                self.previous_chapter();
                true
            }
            "bracketright" => {
                self.next_chapter();
                true
            }
            "slash" => {
                self.open_search();
                true
            }
            "n" => {
                self.search_next(true);
                true
            }
            "N" => {
                self.search_next(false);
                true
            }
            "m" => {
                self.toggle_bookmark();
                true
            }
            "M" => {
                self.edit_note_current();
                true
            }
            "S" => {
                self.show_study();
                true
            }
            "D" => {
                self.show_download();
                true
            }
            "t" => {
                self.widgets.translation_dropdown.grab_focus();
                true
            }
            "T" | "comma" => {
                self.show_settings();
                true
            }
            "question" => {
                self.show_help();
                true
            }
            "Return" | "KP_Enter" => {
                self.focus_next();
                true
            }
            _ => false,
        };

        if handled {
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    }

    fn download_move(&self, delta: isize) {
        let count = {
            let state = self.state.borrow();
            state
                .catalog
                .as_ref()
                .map(|catalog| {
                    catalog
                        .filter(state.download_language.as_deref(), &state.download_query)
                        .len()
                })
                .unwrap_or(0)
        };
        if count == 0 {
            return;
        }

        let current = self
            .widgets
            .download
            .list
            .selected_row()
            .map(|row| row.index() as usize)
            .unwrap_or_else(|| if delta > 0 { 0 } else { count - 1 });
        select_row(&self.widgets.download.list, step(current, delta, count));
    }

    fn download_activate(&self) {
        let Some(row) = self.widgets.download.list.selected_row() else {
            return;
        };
        self.start_download(row.index() as usize);
    }

    pub(crate) fn handle_download_key(
        &self,
        key: gdk::Key,
        modifiers: gdk::ModifierType,
    ) -> glib::Propagation {
        let name = key.name().map(|name| name.to_string()).unwrap_or_default();

        if self.widgets.download.search.has_focus() || self.widgets.download.language.has_focus() {
            if name == "Escape" {
                self.widgets.download.list.grab_focus();
                return glib::Propagation::Stop;
            }
            return glib::Propagation::Proceed;
        }

        if modifiers.contains(gdk::ModifierType::CONTROL_MASK) {
            return glib::Propagation::Proceed;
        }

        let handled = match name.as_str() {
            "Escape" => {
                if self.state.borrow().download.is_some() {
                    self.cancel_download();
                } else {
                    self.widgets.download.window.close();
                }
                true
            }
            "q" => {
                self.widgets.download.window.close();
                true
            }
            "j" | "Down" => {
                self.download_move(1);
                true
            }
            "k" | "Up" => {
                self.download_move(-1);
                true
            }
            "Return" | "KP_Enter" => {
                self.download_activate();
                true
            }
            "r" => {
                self.refresh_catalog();
                true
            }
            "slash" => {
                self.widgets.download.search.grab_focus();
                true
            }
            "l" => {
                self.widgets.download.language.grab_focus();
                true
            }
            _ => false,
        };

        if handled {
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    }

    pub(crate) fn show_help(&self) {
        let window = gtk::Window::builder()
            .title("Keyboard shortcuts")
            .transient_for(&self.widgets.window)
            .modal(false)
            .default_width(560)
            .default_height(620)
            .build();
        window.add_css_class("oma-window");

        let header = gtk::HeaderBar::new();
        let close = gtk::Button::with_label("Close");
        header.pack_end(&close);
        window.set_titlebar(Some(&header));

        let label = gtk::Label::builder()
            .label(SHORTCUTS)
            .xalign(0.0)
            .yalign(0.0)
            .wrap(true)
            .selectable(true)
            .build();
        label.add_css_class("oma-help");

        let scrolled = gtk::ScrolledWindow::builder()
            .child(&label)
            .vexpand(true)
            .hexpand(true)
            .build();
        scrolled.set_margin_top(16);
        scrolled.set_margin_bottom(16);
        scrolled.set_margin_start(20);
        scrolled.set_margin_end(20);
        window.set_child(Some(&scrolled));

        {
            let window = window.clone();
            close.connect_clicked(move |_| window.close());
        }

        window.present();
    }
}

fn step(index: usize, delta: isize, count: usize) -> usize {
    if count == 0 {
        return 0;
    }

    index.saturating_add_signed(delta).min(count - 1)
}

fn language_options(catalog: &RemoteCatalog) -> Vec<(Option<String>, String, usize)> {
    let mut options = vec![(
        None,
        String::from("All languages"),
        catalog.translations().len(),
    )];

    for language in catalog.languages() {
        let label = if language.native_name.is_empty()
            || language.native_name.eq_ignore_ascii_case(&language.name)
        {
            language.name
        } else {
            format!("{}  ·  {}", language.name, language.native_name)
        };

        options.push((Some(language.code), label, language.count));
    }

    options
}

fn format_bytes(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = KIB * 1024;

    if bytes >= MIB {
        format!("{:.1} MiB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.1} KiB", bytes as f64 / KIB as f64)
    } else {
        format!("{bytes} B")
    }
}

const SHORTCUTS: &str = "\
READER

j / k          Move down or up in the focused pane
h / l, Tab     Move between books, chapters and verses
gg / G         Jump to the first or last item
Ctrl+d / Ctrl+u  Move down or up a page
[ / ]          Previous or next chapter
/              Search the current chapter
n / N          Next or previous search match
m              Toggle a bookmark on the current verse
M              Add or edit a note for the current verse
S              Bookmarks and study panel
D              Add a translation from the Bible API
t              Focus the translation chooser
T or ,         Settings
?              This help
q / Esc        Close a panel or quit

BOOKMARKS & STUDY

j / k          Move through saved verses
Enter          Jump to the selected bookmark
x or Delete    Remove the selected bookmark
M              Edit its note
Esc or S       Close the study panel

BIBLE API

j / k          Move through translations
Enter          Download the selected translation
/              Search translations
l              Change the language filter
r              Refresh the catalogue
Esc            Cancel a download or close

The same reader shortcuts work in the terminal app.";

fn add_row(list: &gtk::ListBox, text: &str) -> gtk::ListBoxRow {
    let label = gtk::Label::builder()
        .label(text)
        .xalign(0.0)
        .margin_top(2)
        .margin_bottom(2)
        .build();
    let row = gtk::ListBoxRow::builder()
        .child(&label)
        .activatable(true)
        .build();
    list.append(&row);
    row
}

fn select_row(list: &gtk::ListBox, index: usize) {
    if let Some(row) = list.row_at_index(index as i32) {
        list.select_row(Some(&row));
    }
}

fn highlight_markup(text: &str, query: &str, background: &str) -> String {
    let query = query.trim();
    if query.is_empty() {
        return glib::markup_escape_text(text).to_string();
    }

    let lower_query = query.to_lowercase();
    if lower_query.is_empty() {
        return glib::markup_escape_text(text).to_string();
    }

    // Lowercasing can change the byte length of a character, so keep a map
    // from every byte in the lowered text back to its original character range.
    let mut lowered = String::new();
    let mut ranges = Vec::new();
    for (start, character) in text.char_indices() {
        let end = start + character.len_utf8();
        for lower in character.to_lowercase() {
            for _ in 0..lower.len_utf8() {
                ranges.push((start, end));
            }
            lowered.push(lower);
        }
    }

    let mut result = String::new();
    let mut cursor = 0;
    let mut original_cursor = 0;

    while let Some(found) = lowered[cursor..].find(&lower_query) {
        let start = cursor + found;
        let end = start + lower_query.len();
        let original_start = ranges[start].0;
        let original_end = ranges[end - 1].1;

        result.push_str(&glib::markup_escape_text(
            &text[original_cursor..original_start],
        ));
        result.push_str(&format!(
            "<span background=\"{background}\">{}</span>",
            glib::markup_escape_text(&text[original_start..original_end])
        ));
        cursor = end;
        original_cursor = original_end;
    }

    result.push_str(&glib::markup_escape_text(&text[original_cursor..]));
    result
}
