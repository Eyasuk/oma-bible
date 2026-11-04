use std::{
    sync::mpsc::{Receiver, Sender, TryRecvError},
    thread,
};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use oma_bible_core::{
    bible::{BibleBook, Chapter, Library, Translation},
    fonts,
    remote::{
        DownloadControl, RemoteCatalog, RemoteTranslation, download_translation, save_download,
    },
    storage::{Bookmark, Bookmarks, Settings},
    theme::{Theme, ThemeCatalog},
    user_data_dir,
};

/// Messages sent from worker threads back to the UI.
pub(crate) enum WorkerMessage {
    Catalog(Result<RemoteCatalog, String>),
    Download(Result<DownloadDone, String>),
}

pub(crate) struct DownloadDone {
    pub id: String,
    pub name: String,
    pub font_notice: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Screen {
    Reader,
    Translations,
    Themes,
    Download,
    Languages,
    Study,
    Settings,
    Help,
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InputKind {
    Search,
    Note,
    TranslationFilter,
    DownloadFilter,
    LanguageFilter,
}

pub(crate) struct InputPrompt {
    pub kind: InputKind,
    pub label: String,
    pub value: String,
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

pub(crate) struct LanguageOption {
    pub code: Option<String>,
    pub label: String,
    pub count: usize,
}

pub(crate) struct App {
    pub library: Library,
    pub themes: ThemeCatalog,
    pub settings: Settings,
    pub bookmarks: Bookmarks,

    pub translation_index: usize,
    pub book_index: usize,
    pub chapter_index: usize,
    pub verse_index: usize,
    pub scroll: u16,
    pub focus: Focus,
    pub screen: Screen,

    pub theme_selected: usize,
    pub translation_selected: usize,
    pub translation_query: String,
    pub download_selected: usize,
    pub download_query: String,
    pub download_language: Option<String>,
    pub language_selected: usize,
    pub language_query: String,
    pub study_selected: usize,
    pub settings_selected: usize,

    pub input: Option<InputPrompt>,
    pub note_target: Option<NoteTarget>,
    pending_g: bool,

    pub search_query: String,
    pub search_matches: Vec<usize>,
    pub search_index: usize,

    pub catalog: Option<RemoteCatalog>,
    pub catalog_loading: bool,
    pub catalog_error: Option<String>,
    pub download: Option<DownloadState>,
    pub message: Option<String>,

    pub visible_verses: usize,

    theme_return: Screen,
    translation_return: Screen,
    download_return: Screen,

    worker_tx: Sender<WorkerMessage>,
    worker_rx: Receiver<WorkerMessage>,
    pub quit: bool,
}

impl App {
    pub(crate) fn new(
        library: Library,
        themes: ThemeCatalog,
        settings: Settings,
        bookmarks: Bookmarks,
        worker_tx: Sender<WorkerMessage>,
        worker_rx: Receiver<WorkerMessage>,
    ) -> Self {
        let download_language = settings.download_language.clone();
        let mut app = Self {
            library,
            themes,
            settings,
            bookmarks,
            translation_index: 0,
            book_index: 0,
            chapter_index: 0,
            verse_index: 0,
            scroll: 0,
            focus: Focus::Books,
            screen: Screen::Reader,
            theme_selected: 0,
            translation_selected: 0,
            translation_query: String::new(),
            download_selected: 0,
            download_query: String::new(),
            download_language,
            language_selected: 0,
            language_query: String::new(),
            study_selected: 0,
            settings_selected: 0,
            input: None,
            note_target: None,
            pending_g: false,
            search_query: String::new(),
            search_matches: Vec::new(),
            search_index: 0,
            catalog: None,
            catalog_loading: false,
            catalog_error: None,
            download: None,
            message: None,
            visible_verses: 10,
            theme_return: Screen::Reader,
            translation_return: Screen::Reader,
            download_return: Screen::Reader,
            worker_tx,
            worker_rx,
            quit: false,
        };

        app.restore_position();

        if !app.themes.is_on_omarchy()
            && let Some(slug) = app.settings.theme.clone()
        {
            app.themes.select_slug(&slug);
        }

        app.theme_selected = app.themes.current_index().unwrap_or(0);
        app.translation_selected = app.translation_index;
        app
    }

    fn restore_position(&mut self) {
        let translation = self.settings.translation.clone();
        let book = self.settings.book.clone();
        let chapter = self.settings.chapter;
        let verse = self.settings.verse;

        if let Some(abbreviation) = translation
            && let Some(index) = self
                .library
                .translations()
                .iter()
                .position(|item| item.abbreviation.eq_ignore_ascii_case(&abbreviation))
        {
            self.translation_index = index;
        }

        if let Some(name) = book {
            let index = self
                .translation()
                .and_then(|translation| {
                    translation
                        .books
                        .iter()
                        .position(|item| item.name.eq_ignore_ascii_case(&name))
                })
                .unwrap_or(0);
            self.book_index = index;
        }

        if let Some(number) = chapter {
            let index = self
                .book()
                .and_then(|book| {
                    book.chapters
                        .iter()
                        .position(|chapter| chapter.number == number)
                })
                .unwrap_or(0);
            self.chapter_index = index;
        }

        if let Some(number) = verse
            && let Some(chapter) = self.chapter()
        {
            self.verse_index = (number as usize)
                .saturating_sub(1)
                .min(chapter.verse_count().saturating_sub(1));
        }
    }

    pub(crate) fn theme(&self) -> &Theme {
        self.themes.active()
    }

    /// Follow an Omarchy theme change while the reader is running.
    pub(crate) fn refresh_theme(&mut self) {
        if self.themes.refresh_active() {
            self.theme_selected = self.themes.current_index().unwrap_or(0);
        }
    }

    pub(crate) fn translation(&self) -> Option<&Translation> {
        self.library.translations().get(self.translation_index)
    }

    pub(crate) fn book(&self) -> Option<&BibleBook> {
        self.translation()?.books.get(self.book_index)
    }

    pub(crate) fn chapter(&self) -> Option<&Chapter> {
        self.book()?.chapters.get(self.chapter_index)
    }

    pub(crate) fn current_reference(&self) -> Option<(String, String, u32, u32)> {
        let translation = self.translation()?;
        let book = self.book()?;
        let chapter = self.chapter()?;

        Some((
            translation.abbreviation.clone(),
            book.name.clone(),
            chapter.number,
            self.verse_index as u32 + 1,
        ))
    }

    pub(crate) fn set_translation(&mut self, index: usize) {
        if self.library.translations().is_empty() {
            return;
        }

        self.translation_index = index.min(self.library.translations().len() - 1);
        self.book_index = 0;
        self.chapter_index = 0;
        self.verse_index = 0;
        self.scroll = 0;
        self.clear_search();
    }

    fn select_book(&mut self, delta: isize) {
        let count = self.translation().map(|item| item.books.len()).unwrap_or(0);
        if count == 0 {
            return;
        }

        self.book_index = step(self.book_index, delta, count);
        self.chapter_index = 0;
        self.verse_index = 0;
        self.scroll = 0;
        self.clear_search();
    }

    fn select_chapter(&mut self, delta: isize) {
        let count = self.book().map(|item| item.chapters.len()).unwrap_or(0);
        if count == 0 {
            return;
        }

        self.chapter_index = step(self.chapter_index, delta, count);
        self.verse_index = 0;
        self.scroll = 0;
        self.clear_search();
    }

    fn select_verse(&mut self, delta: isize) {
        let count = self.chapter().map(Chapter::verse_count).unwrap_or(0);
        if count == 0 {
            return;
        }

        self.verse_index = step(self.verse_index, delta, count);
    }

    fn select_book_at(&mut self, index: usize) {
        let count = self.translation().map(|item| item.books.len()).unwrap_or(0);
        if count == 0 {
            return;
        }

        self.book_index = index.min(count - 1);
        self.chapter_index = 0;
        self.verse_index = 0;
        self.scroll = 0;
        self.clear_search();
    }

    fn select_chapter_at(&mut self, index: usize) {
        let count = self.book().map(|item| item.chapters.len()).unwrap_or(0);
        if count == 0 {
            return;
        }

        self.chapter_index = index.min(count - 1);
        self.verse_index = 0;
        self.scroll = 0;
        self.clear_search();
    }

    fn previous_chapter(&mut self) {
        if self.chapter_index > 0 {
            self.chapter_index -= 1;
            self.verse_index = 0;
            self.scroll = 0;
            self.clear_search();
            return;
        }

        let mut book = self.book_index;
        while book > 0 {
            book -= 1;
            let chapters = self
                .translation()
                .and_then(|translation| translation.books.get(book))
                .map(|book| book.chapters.len())
                .unwrap_or(0);

            if chapters > 0 {
                self.book_index = book;
                self.chapter_index = chapters - 1;
                self.verse_index = 0;
                self.scroll = 0;
                self.clear_search();
                return;
            }
        }
    }

    fn next_chapter(&mut self) {
        let chapters = self.book().map(|book| book.chapters.len()).unwrap_or(0);
        if self.chapter_index + 1 < chapters {
            self.chapter_index += 1;
            self.verse_index = 0;
            self.scroll = 0;
            self.clear_search();
            return;
        }

        let books = self.translation().map(|item| item.books.len()).unwrap_or(0);
        let mut book = self.book_index + 1;
        while book < books {
            let chapters = self
                .translation()
                .and_then(|translation| translation.books.get(book))
                .map(|book| book.chapters.len())
                .unwrap_or(0);

            if chapters > 0 {
                self.book_index = book;
                self.chapter_index = 0;
                self.verse_index = 0;
                self.scroll = 0;
                self.clear_search();
                return;
            }

            book += 1;
        }
    }

    fn move_up(&mut self) {
        match self.focus {
            Focus::Books => self.select_book(-1),
            Focus::Chapters => self.select_chapter(-1),
            Focus::Verses => self.select_verse(-1),
        }
    }

    fn move_down(&mut self) {
        match self.focus {
            Focus::Books => self.select_book(1),
            Focus::Chapters => self.select_chapter(1),
            Focus::Verses => self.select_verse(1),
        }
    }

    fn page(&mut self, direction: isize) {
        if self.screen != Screen::Reader {
            let amount = 10 * direction;
            self.move_screen_selection(amount);
            return;
        }

        let amount = (self.visible_verses.max(2) / 2).max(1) as isize * direction;
        match self.focus {
            Focus::Books => self.select_book(amount),
            Focus::Chapters => self.select_chapter(amount),
            Focus::Verses => self.select_verse(amount),
        }
    }

    fn go_top(&mut self) {
        match self.screen {
            Screen::Reader => match self.focus {
                Focus::Books => self.select_book_at(0),
                Focus::Chapters => self.select_chapter_at(0),
                Focus::Verses => {
                    if self.chapter().is_some() {
                        self.verse_index = 0;
                    }
                }
            },
            _ => {
                if self.screen_item_count() > 0 {
                    self.set_screen_selected(0);
                }
            }
        }
    }

    fn go_bottom(&mut self) {
        match self.screen {
            Screen::Reader => match self.focus {
                Focus::Books => {
                    let count = self.translation().map(|item| item.books.len()).unwrap_or(0);
                    self.select_book_at(count.saturating_sub(1));
                }
                Focus::Chapters => {
                    let count = self.book().map(|item| item.chapters.len()).unwrap_or(0);
                    self.select_chapter_at(count.saturating_sub(1));
                }
                Focus::Verses => {
                    if let Some(chapter) = self.chapter() {
                        self.verse_index = chapter.verse_count().saturating_sub(1);
                    }
                }
            },
            _ => {
                let count = self.screen_item_count();
                if count > 0 {
                    self.set_screen_selected(count - 1);
                }
            }
        }
    }

    pub(crate) fn save_position(&mut self) {
        let translation = self.translation().map(|item| item.abbreviation.clone());
        let book = self.book().map(|item| item.name.clone());
        let chapter = self.chapter().map(|item| item.number);
        let verse = self.chapter().map(|_| {
            u32::try_from(self.verse_index)
                .unwrap_or(u32::MAX - 1)
                .saturating_add(1)
        });

        self.settings.translation = translation;
        self.settings.book = book;
        self.settings.chapter = chapter;
        self.settings.verse = verse;

        if !self.themes.is_on_omarchy() {
            self.settings.theme = Some(self.themes.active().slug.clone());
        }

        if let Err(error) = self.settings.save() {
            self.message = Some(format!("Could not save settings: {error}"));
        }
    }

    fn clear_search(&mut self) {
        self.search_query.clear();
        self.search_matches.clear();
        self.search_index = 0;
    }

    pub(crate) fn handle_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }

        self.message = None;

        if self.input.is_some() {
            self.handle_input_key(key);
            return;
        }

        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('d') => self.page(1),
                KeyCode::Char('u') => self.page(-1),
                _ => {}
            }
            return;
        }

        if key.code == KeyCode::Char('g') && key.modifiers.is_empty() {
            if self.pending_g {
                self.pending_g = false;
                self.go_top();
            } else {
                self.pending_g = true;
            }
            return;
        }

        self.pending_g = false;

        if self.screen == Screen::Reader {
            self.handle_reader_key(key);
        } else {
            self.handle_screen_key(key);
        }
    }

    fn handle_reader_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Esc => {
                if self.search_query.is_empty() && self.search_matches.is_empty() {
                    self.quit = true;
                } else {
                    self.clear_search();
                }
            }
            KeyCode::Char('t') => self.open_translations(Screen::Reader),
            KeyCode::Char('T') => self.open_themes(Screen::Reader),
            KeyCode::Char('D') => self.open_download(Screen::Reader),
            KeyCode::Char('S') => {
                self.screen = Screen::Study;
                self.study_selected = 0;
            }
            KeyCode::Char(',') => {
                self.screen = Screen::Settings;
                self.settings_selected = 0;
            }
            KeyCode::Char('?') => self.screen = Screen::Help,
            KeyCode::Char('m') => self.toggle_bookmark(),
            KeyCode::Char('M') => self.open_note_for_current(),
            KeyCode::Char('/') => {
                let value = self.search_query.clone();
                self.open_input(InputKind::Search, "Search this chapter", value);
            }
            KeyCode::Char('n') => self.next_match(true),
            KeyCode::Char('N') => self.next_match(false),
            KeyCode::Char('[') => self.previous_chapter(),
            KeyCode::Char(']') => self.next_chapter(),
            KeyCode::Char('h') | KeyCode::Left => self.focus = self.focus.previous(),
            KeyCode::Char('l') | KeyCode::Right | KeyCode::Tab => self.focus = self.focus.next(),
            KeyCode::BackTab => self.focus = self.focus.previous(),
            KeyCode::Char('j') | KeyCode::Down => self.move_down(),
            KeyCode::Char('k') | KeyCode::Up => self.move_up(),
            KeyCode::Char('G') | KeyCode::End => self.go_bottom(),
            KeyCode::Home => self.go_top(),
            KeyCode::PageDown => self.page(1),
            KeyCode::PageUp => self.page(-1),
            KeyCode::Enter => self.focus = self.focus.next(),
            _ => {}
        }
    }

    fn handle_screen_key(&mut self, key: KeyEvent) {
        if self.screen == Screen::Download && self.download.is_some() {
            if key.code == KeyCode::Esc {
                if let Some(download) = &self.download {
                    download.control.cancel();
                }
                self.message = Some(String::from("Cancelling download…"));
            }
            return;
        }

        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => self.close_screen(),
            KeyCode::Char('j') | KeyCode::Down => self.move_screen_selection(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_screen_selection(-1),
            KeyCode::Char('G') | KeyCode::End => self.go_bottom(),
            KeyCode::Home => self.go_top(),
            KeyCode::PageDown => self.move_screen_selection(10),
            KeyCode::PageUp => self.move_screen_selection(-10),
            KeyCode::Enter => self.activate_screen_item(),
            KeyCode::Char('/') => self.open_screen_filter(),
            KeyCode::Char('r') if self.screen == Screen::Download => self.refresh_catalog(),
            KeyCode::Char('l') if self.screen == Screen::Download => self.open_languages(),
            KeyCode::Char('d') | KeyCode::Char('D') if self.screen == Screen::Translations => {
                self.open_download(Screen::Translations);
            }
            KeyCode::Char('x') if self.screen == Screen::Study => self.remove_selected_bookmark(),
            KeyCode::Char('M') if self.screen == Screen::Study => self.note_selected_bookmark(),
            _ => {}
        }
    }

    fn close_screen(&mut self) {
        self.screen = match self.screen {
            Screen::Themes => self.theme_return,
            Screen::Translations => self.translation_return,
            Screen::Download => self.download_return,
            Screen::Languages => Screen::Download,
            Screen::Study | Screen::Settings | Screen::Help | Screen::Reader => Screen::Reader,
        };
    }

    fn activate_screen_item(&mut self) {
        match self.screen {
            Screen::Themes => self.apply_theme(self.theme_selected),
            Screen::Translations => self.activate_translation(),
            Screen::Download => self.start_selected_download(),
            Screen::Languages => self.apply_language(),
            Screen::Study => self.jump_to_selected_bookmark(),
            Screen::Settings => self.activate_setting(),
            Screen::Help => self.screen = Screen::Reader,
            Screen::Reader => {}
        }
    }

    fn activate_translation(&mut self) {
        let abbreviation = self
            .filtered_translations()
            .get(self.translation_selected)
            .map(|translation| translation.abbreviation.clone());

        if let Some(abbreviation) = abbreviation
            && let Some(index) = self
                .library
                .translations()
                .iter()
                .position(|translation| translation.abbreviation == abbreviation)
        {
            self.set_translation(index);
            self.message = Some(format!(
                "Reading {}",
                self.translation()
                    .map(|translation| translation.name.as_str())
                    .unwrap_or("translation")
            ));
        }

        self.screen = self.translation_return;
    }

    fn activate_setting(&mut self) {
        match self.settings_selected {
            0 => self.open_themes(Screen::Settings),
            1 => {
                self.settings.show_verse_numbers = !self.settings.show_verse_numbers;
                let state = if self.settings.show_verse_numbers {
                    "on"
                } else {
                    "off"
                };
                self.save_settings();
                self.message = Some(format!("Verse numbers {state}"));
            }
            2 => self.open_translations(Screen::Settings),
            3 => self.open_download(Screen::Settings),
            4 => {
                self.screen = Screen::Study;
                self.study_selected = 0;
            }
            5 => self.message = Some(format!("Translations: {}", user_data_dir().display())),
            6 => self.screen = Screen::Help,
            _ => {}
        }
    }

    fn open_screen_filter(&mut self) {
        match self.screen {
            Screen::Translations => {
                let value = self.translation_query.clone();
                self.open_input(InputKind::TranslationFilter, "Filter translations", value);
            }
            Screen::Download => {
                let value = self.download_query.clone();
                self.open_input(InputKind::DownloadFilter, "Search translations", value);
            }
            Screen::Languages => {
                let value = self.language_query.clone();
                self.open_input(InputKind::LanguageFilter, "Search languages", value);
            }
            _ => {}
        }
    }

    fn handle_input_key(&mut self, key: KeyEvent) {
        if key.code == KeyCode::Esc {
            self.input = None;
            return;
        }

        if key.code == KeyCode::Enter {
            if let Some(prompt) = self.input.take() {
                self.commit_input(prompt);
            }
            return;
        }

        if key.modifiers.contains(KeyModifiers::CONTROL) {
            if key.code == KeyCode::Char('u')
                && let Some(prompt) = self.input.as_mut()
            {
                prompt.value.clear();
            }
            return;
        }

        let Some(prompt) = self.input.as_mut() else {
            return;
        };

        match key.code {
            KeyCode::Backspace => {
                prompt.value.pop();
            }
            KeyCode::Char(character) if !key.modifiers.contains(KeyModifiers::ALT) => {
                prompt.value.push(character);
            }
            _ => {}
        }
    }

    fn commit_input(&mut self, prompt: InputPrompt) {
        match prompt.kind {
            InputKind::Search => {
                self.search_query = prompt.value.trim().to_string();
                self.update_search();
                self.message = Some(if self.search_matches.is_empty() {
                    String::from("No matches in this chapter")
                } else {
                    format!(
                        "{} matches for “{}”",
                        self.search_matches.len(),
                        self.search_query
                    )
                });
            }
            InputKind::Note => {
                let Some(target) = self.note_target.take() else {
                    return;
                };

                self.bookmarks.set_note(
                    &target.translation,
                    &target.book,
                    target.chapter,
                    target.verse,
                    &prompt.value,
                );

                if let Err(error) = self.bookmarks.save() {
                    self.message = Some(format!("Could not save bookmark: {error}"));
                } else {
                    self.message = Some(format!(
                        "Saved note for {} {}:{}",
                        target.book, target.chapter, target.verse
                    ));
                }
            }
            InputKind::TranslationFilter => {
                self.translation_query = prompt.value.trim().to_string();
                self.translation_selected = 0;
            }
            InputKind::DownloadFilter => {
                self.download_query = prompt.value.trim().to_string();
                self.download_selected = 0;
            }
            InputKind::LanguageFilter => {
                self.language_query = prompt.value.trim().to_string();
                self.language_selected = 0;
            }
        }
    }

    fn open_input(&mut self, kind: InputKind, label: impl Into<String>, value: String) {
        self.input = Some(InputPrompt {
            kind,
            label: label.into(),
            value,
        });
    }

    fn update_search(&mut self) {
        let query = self.search_query.trim().to_lowercase();
        let matches = if query.is_empty() {
            Vec::new()
        } else {
            self.chapter()
                .map(|chapter| {
                    chapter
                        .verses
                        .iter()
                        .enumerate()
                        .filter(|(_, verse)| verse.to_lowercase().contains(&query))
                        .map(|(index, _)| index)
                        .collect()
                })
                .unwrap_or_default()
        };

        self.search_matches = matches;
        self.search_index = 0;

        if let Some(&index) = self.search_matches.first() {
            self.verse_index = index;
            self.focus = Focus::Verses;
        }
    }

    fn next_match(&mut self, forward: bool) {
        if self.search_matches.is_empty() {
            self.message = Some(String::from("No active search in this chapter"));
            return;
        }

        let count = self.search_matches.len();
        self.search_index = if forward {
            (self.search_index + 1) % count
        } else {
            (self.search_index + count - 1) % count
        };
        self.verse_index = self.search_matches[self.search_index];
        self.focus = Focus::Verses;
    }

    fn toggle_bookmark(&mut self) {
        let Some((translation, book, chapter, verse)) = self.current_reference() else {
            self.message = Some(String::from("No verse selected"));
            return;
        };

        let added = self.bookmarks.toggle(&translation, &book, chapter, verse);

        if let Err(error) = self.bookmarks.save() {
            self.message = Some(format!("Could not save bookmark: {error}"));
            return;
        }

        self.message = Some(if added {
            format!("Bookmarked {book} {chapter}:{verse}")
        } else {
            format!("Removed bookmark {book} {chapter}:{verse}")
        });
    }

    fn open_note_for_current(&mut self) {
        let Some((translation, book, chapter, verse)) = self.current_reference() else {
            self.message = Some(String::from("No verse selected"));
            return;
        };

        let note = self
            .bookmarks
            .find(&translation, &book, chapter, verse)
            .map(|bookmark| bookmark.note.clone())
            .unwrap_or_default();

        self.note_target = Some(NoteTarget {
            translation,
            book: book.clone(),
            chapter,
            verse,
        });
        self.open_input(
            InputKind::Note,
            format!("Note · {book} {chapter}:{verse}"),
            note,
        );
    }

    fn note_selected_bookmark(&mut self) {
        let bookmark = self
            .bookmarks
            .items()
            .iter()
            .rev()
            .nth(self.study_selected)
            .cloned();

        if let Some(bookmark) = bookmark {
            self.note_target = Some(NoteTarget {
                translation: bookmark.translation.clone(),
                book: bookmark.book.clone(),
                chapter: bookmark.chapter,
                verse: bookmark.verse,
            });
            self.open_input(
                InputKind::Note,
                format!("Note · {}", bookmark.reference()),
                bookmark.note,
            );
        }
    }

    fn remove_selected_bookmark(&mut self) {
        let count = self.bookmarks.len();
        if count == 0 {
            return;
        }

        let actual = count - 1 - self.study_selected.min(count - 1);
        if let Some(removed) = self.bookmarks.remove_index(actual) {
            let _ = self.bookmarks.save();
            self.study_selected = self
                .study_selected
                .min(self.bookmarks.len().saturating_sub(1));
            self.message = Some(format!("Removed {}", removed.reference()));
        }
    }

    fn jump_to_selected_bookmark(&mut self) {
        let bookmark = self
            .bookmarks
            .items()
            .iter()
            .rev()
            .nth(self.study_selected)
            .cloned();

        if let Some(bookmark) = bookmark {
            self.jump_to(&bookmark);
        }
    }

    fn jump_to(&mut self, bookmark: &Bookmark) {
        let Some((translation_index, book_index, chapter_index, verse_count)) =
            self.locate(bookmark)
        else {
            self.message = Some(format!("{} is no longer available", bookmark.reference()));
            return;
        };

        self.translation_index = translation_index;
        self.book_index = book_index;
        self.chapter_index = chapter_index;
        self.verse_index = (bookmark.verse as usize)
            .saturating_sub(1)
            .min(verse_count.saturating_sub(1));
        self.focus = Focus::Verses;
        self.screen = Screen::Reader;
        self.clear_search();
        self.message = Some(format!(
            "Jumped to {} {}",
            bookmark.translation,
            bookmark.reference()
        ));
    }

    fn locate(&self, bookmark: &Bookmark) -> Option<(usize, usize, usize, usize)> {
        let translation_index = self.library.translations().iter().position(|translation| {
            translation
                .abbreviation
                .eq_ignore_ascii_case(&bookmark.translation)
        })?;
        let translation = &self.library.translations()[translation_index];
        let book_index = translation
            .books
            .iter()
            .position(|book| book.name.eq_ignore_ascii_case(&bookmark.book))?;
        let book = &translation.books[book_index];
        let chapter_index = book
            .chapters
            .iter()
            .position(|chapter| chapter.number == bookmark.chapter)?;
        let verse_count = book.chapters[chapter_index].verse_count();

        (verse_count > 0).then_some((translation_index, book_index, chapter_index, verse_count))
    }

    pub(crate) fn open_themes(&mut self, from: Screen) {
        self.theme_return = from;
        self.theme_selected = self.themes.current_index().unwrap_or(0);
        self.screen = Screen::Themes;
    }

    fn apply_theme(&mut self, index: usize) {
        if self.themes.themes().is_empty() {
            return;
        }

        let index = index.min(self.themes.themes().len() - 1);
        if self.themes.select(index) {
            if !self.themes.is_on_omarchy() {
                self.settings.theme = Some(self.themes.active().slug.clone());
                self.save_settings();
            }
            self.theme_selected = index;
            self.message = Some(format!("Theme: {}", self.themes.active().name));
        }

        self.screen = self.theme_return;
    }

    fn open_translations(&mut self, from: Screen) {
        self.translation_return = from;
        self.translation_query.clear();
        self.translation_selected = self.translation_index;
        self.screen = Screen::Translations;
    }

    pub(crate) fn open_download(&mut self, from: Screen) {
        self.download_return = from;
        self.download_selected = 0;
        self.screen = Screen::Download;
        self.fetch_catalog();
    }

    fn open_languages(&mut self) {
        if self.catalog.is_none() {
            self.message = Some(String::from("Translations are still loading"));
            return;
        }

        self.language_query.clear();
        let options = self.language_options();
        self.language_selected = match &self.download_language {
            Some(code) => options
                .iter()
                .position(|option| option.code.as_deref() == Some(code.as_str()))
                .unwrap_or(0),
            None => 0,
        };
        self.screen = Screen::Languages;
    }

    fn apply_language(&mut self) {
        let options = self.language_options();
        let Some(option) = options.get(self.language_selected) else {
            return;
        };

        self.download_language = option.code.clone();
        self.settings.download_language = option.code.clone();
        self.download_selected = 0;
        self.screen = Screen::Download;
        self.save_settings();
    }

    fn refresh_catalog(&mut self) {
        if self.catalog_loading {
            return;
        }

        self.catalog = None;
        self.catalog_error = None;
        self.fetch_catalog();
    }

    fn fetch_catalog(&mut self) {
        if self.catalog.is_some() || self.catalog_loading {
            return;
        }

        self.catalog_loading = true;
        self.catalog_error = None;
        let sender = self.worker_tx.clone();

        thread::spawn(move || {
            let result = RemoteCatalog::fetch().map_err(|error| error.to_string());
            let _ = sender.send(WorkerMessage::Catalog(result));
        });
    }

    fn start_selected_download(&mut self) {
        if self.download.is_some() {
            return;
        }

        let selection = self
            .filtered_remote()
            .get(self.download_selected)
            .map(|translation| (translation.id.clone(), translation.title().to_string()));
        let Some((id, name)) = selection else {
            self.message = Some(String::from("Choose a translation first"));
            return;
        };

        let control = DownloadControl::new();
        self.download = Some(DownloadState {
            id: id.clone(),
            name,
            control: control.clone(),
        });

        let sender = self.worker_tx.clone();
        thread::spawn(move || {
            let result = download_translation(&id, &control)
                .and_then(|translation| {
                    let sample = fonts::sample_text(&translation);
                    save_download(&translation, &id)?;
                    let font_notice = fonts::prepare_for_text(&sample, true);
                    Ok(DownloadDone {
                        id: id.clone(),
                        name: translation.name,
                        font_notice,
                    })
                })
                .map_err(|error| error.to_string());
            let _ = sender.send(WorkerMessage::Download(result));
        });
    }

    pub(crate) fn poll_worker(&mut self) {
        loop {
            match self.worker_rx.try_recv() {
                Ok(WorkerMessage::Catalog(result)) => {
                    self.catalog_loading = false;
                    match result {
                        Ok(catalog) => {
                            self.catalog = Some(catalog);
                            self.catalog_error = None;
                            self.download_selected = 0;
                        }
                        Err(error) => self.catalog_error = Some(error),
                    }
                }
                Ok(WorkerMessage::Download(result)) => {
                    let was_cancelled = self
                        .download
                        .as_ref()
                        .is_some_and(|download| download.control.is_cancelled());
                    self.download = None;

                    match result {
                        Ok(done) => {
                            self.library = Library::load_default();
                            if let Some(index) =
                                self.library.translations().iter().position(|translation| {
                                    translation.abbreviation.eq_ignore_ascii_case(&done.id)
                                })
                            {
                                self.set_translation(index);
                            }
                            self.screen = Screen::Reader;
                            self.save_position();
                            let mut message = format!("Added {} from bible.helloao.org", done.name);
                            if let Some(notice) = done.font_notice {
                                message.push_str("  ·  ");
                                message.push_str(&notice);
                            }
                            self.message = Some(message);
                        }
                        Err(error) => {
                            self.message = Some(if was_cancelled {
                                String::from("Download cancelled")
                            } else {
                                format!("Download failed: {error}")
                            });
                        }
                    }
                }
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            }
        }
    }

    fn save_settings(&mut self) {
        if let Err(error) = self.settings.save() {
            self.message = Some(format!("Could not save settings: {error}"));
        }
    }

    pub(crate) fn filtered_translations(&self) -> Vec<&Translation> {
        let query = self.translation_query.trim().to_lowercase();

        self.library
            .translations()
            .iter()
            .filter(|translation| {
                query.is_empty()
                    || [
                        translation.name.as_str(),
                        translation.abbreviation.as_str(),
                        translation.language.as_str(),
                    ]
                    .iter()
                    .any(|value| value.to_lowercase().contains(&query))
            })
            .collect()
    }

    pub(crate) fn filtered_remote(&self) -> Vec<&RemoteTranslation> {
        self.catalog
            .as_ref()
            .map(|catalog| catalog.filter(self.download_language.as_deref(), &self.download_query))
            .unwrap_or_default()
    }

    pub(crate) fn language_options(&self) -> Vec<LanguageOption> {
        let Some(catalog) = &self.catalog else {
            return Vec::new();
        };

        let query = self.language_query.trim().to_lowercase();
        let mut options = Vec::new();

        if query.is_empty() {
            options.push(LanguageOption {
                code: None,
                label: String::from("All languages"),
                count: catalog.translations().len(),
            });
        }

        for language in catalog.languages() {
            if !query.is_empty()
                && !language.name.to_lowercase().contains(&query)
                && !language.native_name.to_lowercase().contains(&query)
                && !language.code.to_lowercase().contains(&query)
            {
                continue;
            }

            let label = if language.native_name.is_empty()
                || language.native_name.eq_ignore_ascii_case(&language.name)
            {
                language.name
            } else {
                format!("{}  ·  {}", language.name, language.native_name)
            };

            options.push(LanguageOption {
                code: Some(language.code),
                label,
                count: language.count,
            });
        }

        options
    }

    fn screen_item_count(&self) -> usize {
        match self.screen {
            Screen::Themes => self.themes.themes().len(),
            Screen::Translations => self.filtered_translations().len(),
            Screen::Download => self.filtered_remote().len(),
            Screen::Languages => self.language_options().len(),
            Screen::Study => self.bookmarks.len(),
            Screen::Settings => SETTINGS_ROWS,
            Screen::Reader | Screen::Help => 0,
        }
    }

    fn screen_selected(&self) -> usize {
        match self.screen {
            Screen::Themes => self.theme_selected,
            Screen::Translations => self.translation_selected,
            Screen::Download => self.download_selected,
            Screen::Languages => self.language_selected,
            Screen::Study => self.study_selected,
            Screen::Settings => self.settings_selected,
            Screen::Reader | Screen::Help => 0,
        }
    }

    fn set_screen_selected(&mut self, value: usize) {
        let count = self.screen_item_count();
        let value = value.min(count.saturating_sub(1));

        match self.screen {
            Screen::Themes => self.theme_selected = value,
            Screen::Translations => self.translation_selected = value,
            Screen::Download => self.download_selected = value,
            Screen::Languages => self.language_selected = value,
            Screen::Study => self.study_selected = value,
            Screen::Settings => self.settings_selected = value.min(SETTINGS_ROWS - 1),
            Screen::Reader | Screen::Help => {}
        }
    }

    fn move_screen_selection(&mut self, delta: isize) {
        let count = self.screen_item_count();
        if count == 0 {
            return;
        }

        let next = step(self.screen_selected(), delta, count);
        self.set_screen_selected(next);
    }
}

const SETTINGS_ROWS: usize = 7;

pub(crate) fn step(index: usize, delta: isize, count: usize) -> usize {
    if count == 0 {
        return 0;
    }

    index.saturating_add_signed(delta).min(count - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_library() -> Library {
        let mut library = Library::new();
        library.add(Translation::new(
            "First",
            "English",
            "FST",
            vec![
                BibleBook::new(
                    "Genesis",
                    vec![
                        Chapter::new(
                            1,
                            vec![
                                String::from("one"),
                                String::from("two"),
                                String::from("one"),
                            ],
                        ),
                        Chapter::new(2, vec![String::from("second chapter")]),
                    ],
                ),
                BibleBook::new(
                    "Exodus",
                    vec![Chapter::new(1, vec![String::from("exodus")])],
                ),
            ],
        ));
        library.add(Translation::new("Second", "English", "SND", Vec::new()));
        library
    }

    fn sample_app() -> App {
        let (sender, receiver) = std::sync::mpsc::channel();
        App::new(
            sample_library(),
            ThemeCatalog::fallback_only(),
            Settings::default(),
            Bookmarks::default(),
            sender,
            receiver,
        )
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn step_clamps_at_both_ends() {
        assert_eq!(step(0, -1, 3), 0);
        assert_eq!(step(2, 1, 3), 2);
        assert_eq!(step(1, 1, 3), 2);
        assert_eq!(step(0, -1, 0), 0);
    }

    #[test]
    fn restores_the_saved_position() {
        let settings = Settings {
            translation: Some(String::from("FST")),
            book: Some(String::from("Exodus")),
            chapter: Some(1),
            verse: Some(1),
            ..Settings::default()
        };

        let (sender, receiver) = std::sync::mpsc::channel();
        let app = App::new(
            sample_library(),
            ThemeCatalog::fallback_only(),
            settings,
            Bookmarks::default(),
            sender,
            receiver,
        );

        assert_eq!(app.translation_index, 0);
        assert_eq!(app.book_index, 1);
        assert_eq!(app.chapter_index, 0);
        assert_eq!(app.verse_index, 0);
    }

    #[test]
    fn gg_and_shift_g_jump_within_the_chapter() {
        let mut app = sample_app();
        app.focus = Focus::Verses;
        app.verse_index = 1;

        app.handle_key(key(KeyCode::Char('g')));
        app.handle_key(key(KeyCode::Char('g')));
        assert_eq!(app.verse_index, 0);

        app.handle_key(key(KeyCode::Char('G')));
        assert_eq!(app.verse_index, 2);
    }

    #[test]
    fn ctrl_d_moves_by_half_a_page() {
        let mut app = sample_app();
        app.focus = Focus::Verses;
        app.visible_verses = 4;
        app.verse_index = 0;

        app.handle_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL));

        assert_eq!(app.verse_index, 2);
    }

    #[test]
    fn search_enters_and_cycles_matches() {
        let mut app = sample_app();

        app.handle_key(key(KeyCode::Char('/')));
        for character in "one".chars() {
            app.handle_key(key(KeyCode::Char(character)));
        }
        app.handle_key(key(KeyCode::Enter));

        assert_eq!(app.search_matches, vec![0, 2]);
        assert_eq!(app.verse_index, 0);

        app.handle_key(key(KeyCode::Char('n')));
        assert_eq!(app.verse_index, 2);

        app.handle_key(key(KeyCode::Char('n')));
        assert_eq!(app.verse_index, 0);
    }

    #[test]
    fn settings_screen_escape_returns_to_the_reader() {
        let mut app = sample_app();

        app.handle_key(key(KeyCode::Char(',')));
        assert_eq!(app.screen, Screen::Settings);
        assert_eq!(app.screen_item_count(), SETTINGS_ROWS);

        app.handle_key(key(KeyCode::Esc));
        assert_eq!(app.screen, Screen::Reader);
    }
}
