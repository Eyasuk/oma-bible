use std::error::Error;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread;

use gpui_kit::assets::{AllAssets, IconName};
use gpui_kit::base::TextSelection;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{ActiveTheme, Icon, h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;
use oma_bible_core::{
    bible::{Library, Translation},
    fonts,
    remote::{self, DownloadControl, RemoteCatalog, RemoteTranslation},
    storage::{Bookmark, Bookmarks, HighlightColor, Highlights, Settings},
    theme::ThemeCatalog,
};

use crate::actions::*;
use crate::chrome;
use crate::reader::ReaderView;
use crate::theme as app_theme;
use crate::tools::Tool;

pub fn run() -> Result<(), Box<dyn Error>> {
    application().with_assets(AllAssets).run(|cx| {
        gpui_kit::component::init(cx);
        bind_keys(cx);

        let library = Library::load_default();
        let mut themes = ThemeCatalog::load();
        let settings = Settings::load();
        if !themes.is_on_omarchy()
            && let Some(slug) = settings.theme.as_deref()
        {
            themes.select_slug(slug);
        }
        let bookmarks = Bookmarks::load();
        let highlights = Highlights::load();

        if let Some(translation) = library.translations().first() {
            let sample = fonts::sample_text(translation);
            let _ = fonts::prepare_for_text(&sample, false);
        }

        let (worker_tx, worker_rx) = mpsc::channel();

        let window_bounds =
            WindowBounds::Windowed(Bounds::centered(None, size(px(1220.), px(840.)), cx));
        let options = WindowOptions {
            window_bounds: Some(window_bounds),
            window_min_size: Some(size(px(940.), px(640.))),
            titlebar: Some(TitlebarOptions {
                title: Some("Oma Bible".into()),
                appears_transparent: false,
                traffic_light_position: None,
            }),
            ..Default::default()
        };

        open_window(options, cx, |window, cx| {
            cx.new(|cx| {
                OmaBibleApp::new(
                    library, settings, bookmarks, highlights, themes, worker_tx, worker_rx, window,
                    cx,
                )
            })
        })
        .expect("failed to open window");

        cx.activate(true);
    });

    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusPane {
    Books,
    Chapters,
    Verses,
}

impl FocusPane {
    pub fn next(self) -> Self {
        match self {
            Self::Books => Self::Chapters,
            Self::Chapters => Self::Verses,
            Self::Verses => Self::Books,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Reader,
    Translations,
    Themes,
    Downloads,
    Languages,
    Study,
    Help,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputKind {
    Search,
    Note,
    TranslationFilter,
    DownloadFilter,
    LanguageFilter,
}

/// Lightweight overlays keep the reader on screen instead of replacing it
/// with a full page. The navigator is the primary way to move around; the
/// settings sheet holds library, appearance and reading preferences.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Overlay {
    None,
    Navigator,
    Settings,
    Highlight,
    NoteDetail,
    Wheel,
}

pub enum WorkerMessage {
    Catalog(Result<RemoteCatalog, String>),
    Download(Result<DownloadDone, String>),
}

pub struct DownloadDone {
    pub id: String,
    pub name: String,
    pub font_notice: Option<String>,
}

pub struct DownloadState {
    pub id: String,
    pub name: String,
    pub control: DownloadControl,
}

pub struct InputPrompt {
    pub kind: InputKind,
    pub state: Entity<InputState>,
}

#[derive(Clone)]
pub struct NoteTarget {
    pub translation: String,
    pub book: String,
    pub chapter: u32,
    pub verse: u32,
}

pub struct OmaBibleApp {
    pub library: Library,
    pub settings: Settings,
    pub bookmarks: Bookmarks,
    pub highlights: Highlights,
    pub themes: ThemeCatalog,
    pub screen: Screen,
    pub focus: FocusPane,
    pub translation_index: usize,
    pub book_index: usize,
    pub chapter_index: usize,
    pub verse_index: usize,
    pub search_query: String,
    pub search_matches: Vec<usize>,
    pub search_index: usize,
    pub status: SharedString,
    pub pending_g: bool,
    /// Anchor of a Vim-style visual selection, as a zero-based verse index.
    pub visual_anchor: Option<usize>,

    // screen selections / state
    pub translation_selected: usize,
    pub translation_query: String,
    pub theme_selected: usize,
    pub download_selected: usize,
    pub download_query: String,
    pub download_language: Option<String>,
    pub language_selected: usize,
    pub language_query: String,
    pub study_selected: usize,
    pub settings_selected: usize,
    pub highlight_selected: usize,

    pub input: Option<InputPrompt>,
    pub note_target: Option<NoteTarget>,
    pub note_detail: Option<NoteTarget>,

    pub overlay: Overlay,
    /// Window position the wheel opens around; `None` centres it.
    pub wheel_center: Option<Point<Pixels>>,
    pub wheel_hover: Option<Tool>,
    /// The chapter notes stack fans out while the pointer is over it.
    pub notes_expanded: bool,
    pub nav_book_index: usize,
    pub nav_chapter_index: usize,
    pub nav_book_scroll: ScrollHandle,
    pub nav_chapter_scroll: ScrollHandle,
    pub reading_scroll: ScrollHandle,
    pub screen_scroll: ScrollHandle,

    pub catalog: Option<RemoteCatalog>,
    pub catalog_loading: bool,
    pub catalog_error: Option<String>,
    pub download: Option<DownloadState>,

    pub theme_return: Screen,
    pub translation_return: Screen,
    pub download_return: Screen,

    worker_tx: Sender<WorkerMessage>,
    worker_rx: Receiver<WorkerMessage>,
    focus_handle: FocusHandle,
}

impl OmaBibleApp {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        library: Library,
        settings: Settings,
        bookmarks: Bookmarks,
        highlights: Highlights,
        mut themes: ThemeCatalog,
        worker_tx: Sender<WorkerMessage>,
        worker_rx: Receiver<WorkerMessage>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let translation_index = settings
            .translation
            .as_deref()
            .and_then(|abbr| {
                library
                    .translations()
                    .iter()
                    .position(|t| t.abbreviation.eq_ignore_ascii_case(abbr))
            })
            .unwrap_or(0);

        // Off Omarchy, start in the palette that matches the desktop's
        // light/dark appearance. A theme picked in the selector wins.
        if !themes.is_on_omarchy() && settings.theme.is_none() {
            let dark = app_theme::system_is_dark(window);
            if let Some(index) = themes
                .themes()
                .iter()
                .position(|theme| theme.mode.is_dark() == dark)
            {
                themes.select(index);
            }
        }

        let focus_handle = cx.focus_handle();
        window.focus(&focus_handle, cx);

        let mut app = Self {
            library,
            settings: settings.clone(),
            bookmarks,
            highlights,
            themes,
            screen: Screen::Reader,
            focus: FocusPane::Verses,
            translation_index,
            book_index: 0,
            chapter_index: 0,
            verse_index: 0,
            search_query: String::new(),
            search_matches: Vec::new(),
            search_index: 0,
            status: "Ready".into(),
            pending_g: false,
            visual_anchor: None,
            translation_selected: translation_index,
            translation_query: String::new(),
            theme_selected: 0,
            download_selected: 0,
            download_query: String::new(),
            download_language: settings.download_language.clone(),
            language_selected: 0,
            language_query: String::new(),
            study_selected: 0,
            settings_selected: 0,
            highlight_selected: 0,
            input: None,
            note_target: None,
            note_detail: None,
            overlay: Overlay::None,
            wheel_center: None,
            wheel_hover: None,
            notes_expanded: false,
            nav_book_index: 0,
            nav_chapter_index: 0,
            nav_book_scroll: ScrollHandle::new(),
            nav_chapter_scroll: ScrollHandle::new(),
            reading_scroll: ScrollHandle::new(),
            screen_scroll: ScrollHandle::new(),
            catalog: None,
            catalog_loading: false,
            catalog_error: None,
            download: None,
            theme_return: Screen::Reader,
            translation_return: Screen::Reader,
            download_return: Screen::Reader,
            worker_tx,
            worker_rx,
            focus_handle,
        };

        app.restore_position();
        app.theme_selected = app.themes.current_index().unwrap_or(0);
        app.translation_selected = app.translation_index;
        app.nav_book_index = app.book_index;
        app.nav_chapter_index = app.chapter_index;
        app.reading_scroll.scroll_to_item(0);

        if app.library.translations().is_empty() {
            app.open_downloads(Screen::Reader, cx);
            app.status = "No translations installed. Open Downloads to add one.".into();
        }

        // `restore_position` restores settings, so the palette is installed
        // once the app owns its theme catalog.
        app.theme_selected = app.themes.current_index().unwrap_or(0);
        app_theme::apply(app.themes.active(), cx);
        app.observe_appearance(window, cx);
        app.watch_omarchy_theme(cx);

        app
    }

    /// Apply the current core palette to gpui-kit's global theme.
    pub fn apply_current_theme(&mut self, cx: &mut Context<Self>) {
        app_theme::apply(self.themes.active(), cx);
        self.theme_selected = self.themes.current_index().unwrap_or(0);
        cx.notify();
    }

    /// Follow Omarchy's active theme while the reader stays open.
    ///
    /// Omarchy swaps the `current/theme` link when the desktop theme changes;
    /// re-reading it keeps the reader in step without a restart. Off Omarchy
    /// the saved selection and system appearance are authoritative instead.
    fn watch_omarchy_theme(&mut self, cx: &mut Context<Self>) {
        if !self.themes.is_on_omarchy() {
            return;
        }

        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(1))
                    .await;
                let Ok(()) = this.update(cx, |this, cx| {
                    if this.themes.refresh_active() {
                        this.apply_current_theme(cx);
                    }
                }) else {
                    break;
                };
            }
        })
        .detach();
    }

    fn observe_appearance(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        cx.observe_window_appearance(window, |this, window, cx| {
            if this.themes.is_on_omarchy() || this.settings.theme.is_some() {
                return;
            }
            let dark = app_theme::system_is_dark(window);
            if let Some(index) = this
                .themes
                .themes()
                .iter()
                .position(|theme| theme.mode.is_dark() == dark)
            {
                this.themes.select(index);
                this.apply_current_theme(cx);
            }
        })
        .detach();
    }

    fn restore_position(&mut self) {
        if let Some(abbr) = self.settings.translation.clone()
            && let Some(i) = self
                .library
                .translations()
                .iter()
                .position(|t| t.abbreviation.eq_ignore_ascii_case(&abbr))
        {
            self.translation_index = i;
        }
        if let Some(book) = self.settings.book.clone()
            && let Some(i) = self.translation().and_then(|t| {
                t.books
                    .iter()
                    .position(|b| b.name.eq_ignore_ascii_case(&book))
            })
        {
            self.book_index = i;
        }
        if let Some(ch) = self.settings.chapter
            && let Some(i) = self
                .book()
                .and_then(|b| b.chapters.iter().position(|c| c.number == ch))
        {
            self.chapter_index = i;
        }
        if let Some(v) = self.settings.verse
            && let Some(chapter) = self.chapter()
        {
            self.verse_index = (v as usize)
                .saturating_sub(1)
                .min(chapter.verse_count().saturating_sub(1));
        }
        self.clamp_indices();
    }

    pub fn translation(&self) -> Option<&Translation> {
        self.library.translations().get(self.translation_index)
    }
    pub fn book(&self) -> Option<&oma_bible_core::bible::BibleBook> {
        self.translation()
            .and_then(|t| t.books.get(self.book_index))
    }
    pub fn chapter(&self) -> Option<&oma_bible_core::bible::Chapter> {
        self.book().and_then(|b| b.chapters.get(self.chapter_index))
    }

    pub fn current_reference(&self) -> Option<(String, String, u32, u32)> {
        let t = self.translation()?;
        let b = self.book()?;
        let c = self.chapter()?;
        Some((
            t.abbreviation.clone(),
            b.name.clone(),
            c.number,
            self.verse_index as u32 + 1,
        ))
    }

    /// Verses that should appear in the reader.
    ///
    /// Some downloaded translations keep verse slots with no text. Empty slots
    /// are skipped so they do not leave blank rows, unless they carry a
    /// bookmark, note, or highlight that still needs to be visible.
    pub fn visible_verse_indices(&self) -> Vec<usize> {
        let Some(translation) = self.translation() else {
            return Vec::new();
        };
        let Some(book) = self.book() else {
            return Vec::new();
        };
        let Some(chapter) = self.chapter() else {
            return Vec::new();
        };
        chapter
            .verses
            .iter()
            .enumerate()
            .filter(|(index, text)| {
                let verse = *index as u32 + 1;
                !text.trim().is_empty()
                    || self
                        .bookmarks
                        .find(&translation.abbreviation, &book.name, chapter.number, verse)
                        .is_some()
                    || self
                        .highlights
                        .find(&translation.abbreviation, &book.name, chapter.number, verse)
                        .is_some()
            })
            .map(|(index, _)| index)
            .collect()
    }

    /// Sorted anchor-to-cursor range while a visual selection is active.
    pub fn visual_range(&self) -> Option<(usize, usize)> {
        self.visual_anchor
            .map(|anchor| (anchor.min(self.verse_index), anchor.max(self.verse_index)))
    }

    /// Number of visible verses covered by the visual selection.
    pub fn visual_verse_count(&self) -> usize {
        let Some((start, end)) = self.visual_range() else {
            return 0;
        };
        self.visible_verse_indices()
            .into_iter()
            .filter(|index| *index >= start && *index <= end)
            .count()
    }

    /// Visible verses an action should apply to: the visual range, or just
    /// the cursor verse.
    fn action_verse_indices(&self) -> Vec<usize> {
        let visible = self.visible_verse_indices();
        if let Some((start, end)) = self.visual_range() {
            visible
                .into_iter()
                .filter(|index| *index >= start && *index <= end)
                .collect()
        } else if visible.contains(&self.verse_index) {
            vec![self.verse_index]
        } else {
            Vec::new()
        }
    }

    pub fn toggle_visual(&mut self, cx: &mut Context<Self>) {
        if self.visual_anchor.take().is_none() {
            self.visual_anchor = Some(self.verse_index);
            let count = self.visual_verse_count().max(1);
            self.set_status(
                format!(
                    "Visual · {count} verse{}",
                    if count == 1 { "" } else { "s" }
                ),
                cx,
            );
        } else {
            self.set_status("Visual off", cx);
        }
    }

    fn clear_visual(&mut self) {
        self.visual_anchor = None;
    }

    /// Scroll the reader to a verse, accounting for skipped empty verses.
    fn scroll_to_verse(&self, verse_index: usize) {
        let visible = self.visible_verse_indices();
        if visible.is_empty() {
            self.reading_scroll.scroll_to_item(0);
            return;
        }
        let position = visible
            .iter()
            .position(|index| *index == verse_index)
            .or_else(|| visible.iter().position(|index| *index > verse_index))
            .unwrap_or(visible.len() - 1);
        // Child 0 is the chapter heading; verse rows follow.
        self.reading_scroll.scroll_to_item(position + 1);
    }

    pub fn clamp_indices(&mut self) {
        if self.library.translations().is_empty() {
            self.translation_index = 0;
            self.book_index = 0;
            self.chapter_index = 0;
            self.verse_index = 0;
            return;
        }
        self.translation_index = self
            .translation_index
            .min(self.library.translations().len().saturating_sub(1));
        let bc = self.translation().map(|t| t.books.len()).unwrap_or(0);
        if bc == 0 {
            self.book_index = 0;
            self.chapter_index = 0;
            self.verse_index = 0;
            return;
        }
        self.book_index = self.book_index.min(bc - 1);
        let cc = self.book().map(|b| b.chapters.len()).unwrap_or(0);
        if cc == 0 {
            self.chapter_index = 0;
            self.verse_index = 0;
            return;
        }
        self.chapter_index = self.chapter_index.min(cc - 1);
        let vc = self.chapter().map(|c| c.verses.len()).unwrap_or(0);
        self.verse_index = if vc == 0 {
            0
        } else {
            self.verse_index.min(vc - 1)
        };
        let visible = self.visible_verse_indices();
        if !visible.is_empty() && !visible.contains(&self.verse_index) {
            self.verse_index = visible
                .iter()
                .copied()
                .find(|index| *index >= self.verse_index)
                .unwrap_or_else(|| *visible.last().expect("visible verses are not empty"));
        }
    }

    pub fn set_status<S: Into<SharedString>>(&mut self, s: S, cx: &mut Context<Self>) {
        self.status = s.into();
        cx.notify();
    }

    pub fn save_position(&mut self) {
        self.settings.translation = self.translation().map(|t| t.abbreviation.clone());
        self.settings.book = self.book().map(|b| b.name.clone());
        self.settings.chapter = self.chapter().map(|c| c.number);
        self.settings.verse = self.chapter().map(|_| self.verse_index as u32 + 1);
        if !self.themes.is_on_omarchy() {
            self.settings.theme = Some(self.themes.active().slug.clone());
        }
        let _ = self.settings.save();
    }

    fn clear_search(&mut self) {
        self.search_query.clear();
        self.search_matches.clear();
        self.search_index = 0;
    }

    fn select_book(&mut self, delta: isize) {
        let Some(t) = self.translation() else {
            return;
        };
        let len = t.books.len();
        if len == 0 {
            return;
        }
        self.book_index = step(self.book_index, delta, len);
        self.chapter_index = 0;
        self.verse_index = 0;
        self.clear_visual();
        self.clear_search();
        self.reading_scroll.scroll_to_item(0);
    }
    fn select_chapter(&mut self, delta: isize) {
        let Some(b) = self.book() else {
            return;
        };
        let len = b.chapters.len();
        if len == 0 {
            return;
        }
        self.chapter_index = step(self.chapter_index, delta, len);
        self.verse_index = 0;
        self.clear_visual();
        self.clear_search();
        self.reading_scroll.scroll_to_item(0);
    }
    fn select_verse(&mut self, delta: isize) {
        let visible = self.visible_verse_indices();
        if visible.is_empty() {
            return;
        }
        let current = visible
            .iter()
            .position(|index| *index == self.verse_index)
            .unwrap_or_else(|| {
                if delta >= 0 {
                    visible
                        .iter()
                        .position(|index| *index > self.verse_index)
                        .unwrap_or(0)
                } else {
                    visible
                        .iter()
                        .rposition(|index| *index < self.verse_index)
                        .unwrap_or(visible.len() - 1)
                }
            });
        let next = current.saturating_add_signed(delta).min(visible.len() - 1);
        self.verse_index = visible[next];
        self.scroll_to_verse(self.verse_index);
    }

    fn move_up(&mut self, cx: &mut Context<Self>) {
        match self.screen {
            Screen::Reader => match self.focus {
                FocusPane::Books => self.select_book(-1),
                FocusPane::Chapters => self.select_chapter(-1),
                FocusPane::Verses => self.select_verse(-1),
            },
            _ => self.move_screen_selection(-1),
        }
        cx.notify();
    }
    fn move_down(&mut self, cx: &mut Context<Self>) {
        match self.screen {
            Screen::Reader => match self.focus {
                FocusPane::Books => self.select_book(1),
                FocusPane::Chapters => self.select_chapter(1),
                FocusPane::Verses => self.select_verse(1),
            },
            _ => self.move_screen_selection(1),
        }
        cx.notify();
    }
    fn move_left(&mut self, cx: &mut Context<Self>) {
        if self.screen == Screen::Reader {
            self.prev_chapter(cx);
        }
    }
    fn move_right(&mut self, cx: &mut Context<Self>) {
        if self.screen == Screen::Reader {
            self.next_chapter(cx);
        }
    }

    fn page(&mut self, dir: isize, cx: &mut Context<Self>) {
        let delta = 10 * dir;
        match self.screen {
            Screen::Reader => match self.focus {
                FocusPane::Books => self.select_book(delta),
                FocusPane::Chapters => self.select_chapter(delta),
                FocusPane::Verses => self.select_verse(delta),
            },
            _ => self.move_screen_selection(delta),
        }
        cx.notify();
    }

    fn go_top(&mut self, cx: &mut Context<Self>) {
        match self.screen {
            Screen::Reader => match self.focus {
                FocusPane::Books => {
                    self.book_index = 0;
                    self.chapter_index = 0;
                    self.verse_index = 0;
                    self.clear_search();
                }
                FocusPane::Chapters => {
                    self.chapter_index = 0;
                    self.verse_index = 0;
                    self.clear_search();
                }
                FocusPane::Verses => {
                    self.verse_index = self.visible_verse_indices().first().copied().unwrap_or(0);
                }
            },
            _ => self.set_screen_selected(0),
        }
        if self.screen == Screen::Reader {
            match self.focus {
                FocusPane::Verses => self.scroll_to_verse(self.verse_index),
                _ => self.reading_scroll.scroll_to_item(0),
            }
        }
        cx.notify();
    }
    fn go_bottom(&mut self, cx: &mut Context<Self>) {
        match self.screen {
            Screen::Reader => match self.focus {
                FocusPane::Books => {
                    self.book_index = self
                        .translation()
                        .map(|t| t.books.len().saturating_sub(1))
                        .unwrap_or(0);
                    self.chapter_index = 0;
                    self.verse_index = 0;
                    self.clear_search();
                }
                FocusPane::Chapters => {
                    self.chapter_index = self
                        .book()
                        .map(|b| b.chapters.len().saturating_sub(1))
                        .unwrap_or(0);
                    self.verse_index = 0;
                    self.clear_search();
                }
                FocusPane::Verses => {
                    self.verse_index = self.visible_verse_indices().last().copied().unwrap_or(0)
                }
            },
            _ => {
                let n = self.screen_item_count().saturating_sub(1);
                self.set_screen_selected(n);
            }
        }
        if self.screen == Screen::Reader {
            self.scroll_to_verse(self.verse_index);
        }
        cx.notify();
    }

    fn prev_chapter(&mut self, cx: &mut Context<Self>) {
        self.clear_visual();
        if self.chapter_index > 0 {
            self.chapter_index -= 1;
            self.verse_index = 0;
            self.clear_search();
        } else if self.book_index > 0 {
            self.book_index -= 1;
            self.chapter_index = self
                .book()
                .map(|b| b.chapters.len().saturating_sub(1))
                .unwrap_or(0);
            self.verse_index = 0;
            self.clear_search();
        }
        self.reading_scroll.scroll_to_item(0);
        cx.notify();
    }
    fn next_chapter(&mut self, cx: &mut Context<Self>) {
        self.clear_visual();
        let ch_count = self.book().map(|b| b.chapters.len()).unwrap_or(0);
        if self.chapter_index + 1 < ch_count {
            self.chapter_index += 1;
            self.verse_index = 0;
            self.clear_search();
        } else {
            let b_count = self.translation().map(|t| t.books.len()).unwrap_or(0);
            if self.book_index + 1 < b_count {
                self.book_index += 1;
                self.chapter_index = 0;
                self.verse_index = 0;
                self.clear_search();
            }
        }
        self.reading_scroll.scroll_to_item(0);
        cx.notify();
    }

    // --- screen navigation/state ---
    pub fn open_translations(&mut self, from: Screen, cx: &mut Context<Self>) {
        self.translation_return = from;
        self.translation_query.clear();
        self.translation_selected = self.translation_index;
        self.screen = Screen::Translations;
        self.screen_scroll.scroll_to_item(self.translation_selected);
        cx.notify();
    }
    pub fn open_themes(&mut self, from: Screen, cx: &mut Context<Self>) {
        self.theme_return = from;
        self.theme_selected = self.themes.current_index().unwrap_or(0);
        self.screen = Screen::Themes;
        self.screen_scroll.scroll_to_item(self.theme_selected);
        cx.notify();
    }
    pub fn open_downloads(&mut self, from: Screen, cx: &mut Context<Self>) {
        self.download_return = from;
        self.screen = Screen::Downloads;
        self.screen_scroll.scroll_to_item(self.download_selected);
        self.fetch_catalog(cx);
        cx.notify();
    }
    pub fn open_study(&mut self, cx: &mut Context<Self>) {
        self.study_selected = 0;
        self.screen = Screen::Study;
        self.screen_scroll.scroll_to_item(0);
        cx.notify();
    }
    pub fn open_help(&mut self, cx: &mut Context<Self>) {
        self.screen = Screen::Help;
        self.screen_scroll.scroll_to_item(0);
        cx.notify();
    }

    pub fn close_screen(&mut self, cx: &mut Context<Self>) {
        self.input = None;
        self.screen = match self.screen {
            Screen::Themes => self.theme_return,
            Screen::Translations => self.translation_return,
            Screen::Downloads | Screen::Languages => self.download_return,
            _ => Screen::Reader,
        };
        if self.screen != Screen::Downloads && self.screen != Screen::Languages {
            // keep download_return sane
        }
        cx.notify();
    }

    // --- minimal reader overlays ---
    pub fn reader_font_size(&self) -> f32 {
        (self.settings.font_size as f32).clamp(26.0, 44.0)
    }

    pub fn open_navigator(&mut self, cx: &mut Context<Self>) {
        self.nav_book_index = self.book_index;
        self.nav_chapter_index = self.chapter_index;
        self.overlay = Overlay::Navigator;
        self.scroll_navigator(cx);
        cx.notify();
    }

    /// Reveal the selected book and chapter in the navigator.
    ///
    /// `scroll_to_item` needs the scroll container's child bounds, which do not
    /// exist until the overlay has been laid out once. The deferred second pass
    /// makes the first `o` (and a book change made during that first frame)
    /// land on the right rows.
    fn scroll_navigator(&self, cx: &mut Context<Self>) {
        let book = self.nav_book_index;
        let chapter = self.nav_chapter_index;
        let book_scroll = self.nav_book_scroll.clone();
        let chapter_scroll = self.nav_chapter_scroll.clone();
        book_scroll.scroll_to_item(book);
        chapter_scroll.scroll_to_item(chapter);
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(16))
                .await;
            this.update(cx, |_, cx| {
                book_scroll.scroll_to_item(book);
                chapter_scroll.scroll_to_item(chapter);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub fn open_settings_sheet(&mut self, cx: &mut Context<Self>) {
        self.settings_selected = self
            .settings_selected
            .min(self.settings_row_count().saturating_sub(1));
        self.overlay = Overlay::Settings;
        cx.notify();
    }

    pub fn close_overlay(&mut self, cx: &mut Context<Self>) {
        if self.overlay != Overlay::None {
            self.overlay = Overlay::None;
            cx.notify();
        }
    }

    fn nav_book(&self) -> Option<&oma_bible_core::bible::BibleBook> {
        self.translation()
            .and_then(|translation| translation.books.get(self.nav_book_index))
    }

    fn nav_chapter_count(&self) -> usize {
        self.nav_book().map(|book| book.chapters.len()).unwrap_or(0)
    }

    fn nav_move_book(&mut self, delta: isize, cx: &mut Context<Self>) {
        let count = self.translation().map(|t| t.books.len()).unwrap_or(0);
        if count == 0 {
            return;
        }
        self.nav_book_index = step(self.nav_book_index, delta, count);
        self.nav_chapter_index = 0;
        self.scroll_navigator(cx);
    }

    fn nav_move_chapter(&mut self, delta: isize, cx: &mut Context<Self>) {
        let count = self.nav_chapter_count();
        if count == 0 {
            return;
        }
        self.nav_chapter_index = step(self.nav_chapter_index, delta, count);
        self.scroll_navigator(cx);
    }

    fn nav_select_book(&mut self, index: usize, cx: &mut Context<Self>) {
        if index >= self.translation().map(|t| t.books.len()).unwrap_or(0) {
            return;
        }
        if self.nav_book_index != index {
            self.nav_book_index = index;
            self.nav_chapter_index = 0;
        }
        self.scroll_navigator(cx);
    }

    fn apply_navigator(&mut self, cx: &mut Context<Self>) {
        let book_count = self.translation().map(|t| t.books.len()).unwrap_or(0);
        if book_count == 0 {
            self.overlay = Overlay::None;
            cx.notify();
            return;
        }
        self.nav_book_index = self.nav_book_index.min(book_count - 1);
        let chapter_count = self.nav_chapter_count();
        self.book_index = self.nav_book_index;
        self.chapter_index = if chapter_count == 0 {
            0
        } else {
            self.nav_chapter_index.min(chapter_count - 1)
        };
        self.verse_index = 0;
        self.overlay = Overlay::None;
        self.screen = Screen::Reader;
        self.clear_search();
        self.clear_visual();
        self.reading_scroll.scroll_to_item(0);
        self.save_position();
        self.status = "Ready".into();
        cx.notify();
    }

    fn activate_settings_sheet_item(&mut self, cx: &mut Context<Self>) {
        match self.settings_selected {
            0 => {
                if self.themes.is_on_omarchy() {
                    self.status =
                        format!("Omarchy controls the theme · {}", self.themes.active().name)
                            .into();
                    cx.notify();
                } else {
                    self.overlay = Overlay::None;
                    self.open_themes(self.screen, cx);
                }
            }
            1 => {
                self.overlay = Overlay::None;
                self.open_translations(self.screen, cx);
            }
            2 => {
                self.overlay = Overlay::None;
                self.open_downloads(self.screen, cx);
            }
            3 => {
                self.overlay = Overlay::None;
                self.open_study(cx);
            }
            4 => {
                self.settings.show_verse_numbers = !self.settings.show_verse_numbers;
                let _ = self.settings.save();
                self.status = format!(
                    "Verse numbers {}",
                    if self.settings.show_verse_numbers {
                        "on"
                    } else {
                        "off"
                    }
                )
                .into();
                cx.notify();
            }
            5 => {
                self.adjust_font_size(2, cx);
            }
            6 => self.cycle_dock(cx),
            7 => {
                self.overlay = Overlay::None;
                self.open_help(cx);
            }
            _ => {}
        }
    }

    fn adjust_font_size(&mut self, delta: i32, cx: &mut Context<Self>) {
        let next = (self.reader_font_size() as i32 + delta).clamp(24, 44);
        self.settings.font_size = next as u32;
        let _ = self.settings.save();
        self.status = format!("Text size {next}px").into();
        cx.notify();
    }

    fn handle_overlay_key(
        &mut self,
        key: &str,
        ctrl: bool,
        shift: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.overlay {
            Overlay::Navigator => match key {
                "escape" | "q" | "o" => self.close_overlay(cx),
                "j" | "down" => {
                    self.nav_move_book(1, cx);
                    cx.notify();
                }
                "k" | "up" => {
                    self.nav_move_book(-1, cx);
                    cx.notify();
                }
                "h" | "left" => {
                    self.nav_move_chapter(-1, cx);
                    cx.notify();
                }
                "l" | "right" | "tab" => {
                    self.nav_move_chapter(1, cx);
                    cx.notify();
                }
                "enter" => self.apply_navigator(cx),
                _ => {}
            },
            Overlay::Settings => match key {
                "escape" | "q" | "," => self.close_overlay(cx),
                "j" | "down" => {
                    self.settings_selected =
                        (self.settings_selected + 1).min(self.settings_row_count() - 1);
                    cx.notify();
                }
                "k" | "up" => {
                    self.settings_selected = self.settings_selected.saturating_sub(1);
                    cx.notify();
                }
                "enter" => self.activate_settings_sheet_item(cx),
                "+" | "=" if !ctrl => self.adjust_font_size(2, cx),
                "-" | "_" => self.adjust_font_size(-2, cx),
                _ => {}
            },
            Overlay::Highlight => {
                let count = HighlightColor::ALL.len() + 1;
                match key {
                    "escape" | "q" => self.close_overlay(cx),
                    "h" if shift => self.close_overlay(cx),
                    "j" | "down" | "l" | "right" | "tab" => {
                        self.highlight_selected = (self.highlight_selected + 1) % count;
                        cx.notify();
                    }
                    "k" | "up" | "h" | "left" => {
                        self.highlight_selected = (self.highlight_selected + count - 1) % count;
                        cx.notify();
                    }
                    "enter" => self.apply_highlight(cx),
                    "x" | "delete" | "backspace" => {
                        self.highlight_selected = HighlightColor::ALL.len();
                        self.apply_highlight(cx);
                    }
                    "1" => self.choose_highlight(0, cx),
                    "2" => self.choose_highlight(1, cx),
                    "3" => self.choose_highlight(2, cx),
                    "4" => self.choose_highlight(3, cx),
                    "5" => self.choose_highlight(HighlightColor::ALL.len(), cx),
                    _ => {}
                }
            }
            Overlay::NoteDetail => match key {
                "escape" | "q" => self.close_overlay(cx),
                "e" | "enter" => self.edit_note_detail(window, cx),
                _ => {}
            },
            Overlay::Wheel => match key {
                "escape" | "q" | "space" => self.close_overlay(cx),
                _ => {
                    let by_number = key
                        .parse::<usize>()
                        .ok()
                        .and_then(|n| n.checked_sub(1))
                        .and_then(|i| Tool::WHEEL.get(i).copied());
                    // GPUI reports Shift+h as "h" with `shift` set.
                    let typed = if shift && key.len() == 1 {
                        key.to_uppercase()
                    } else {
                        key.to_string()
                    };
                    let by_key = Tool::WHEEL.iter().copied().find(|tool| tool.key() == typed);
                    if let Some(tool) = by_number.or(by_key) {
                        self.run_tool(tool, window, cx);
                    }
                }
            },
            Overlay::None => {}
        }
    }

    pub fn filtered_translations(&self) -> Vec<&Translation> {
        let q = self.translation_query.trim().to_lowercase();
        self.library
            .translations()
            .iter()
            .filter(|t| {
                q.is_empty()
                    || [
                        t.name.as_str(),
                        t.abbreviation.as_str(),
                        t.language.as_str(),
                    ]
                    .iter()
                    .any(|v| v.to_lowercase().contains(&q))
            })
            .collect()
    }
    pub fn filtered_remote(&self) -> Vec<&RemoteTranslation> {
        self.catalog
            .as_ref()
            .map(|c| c.filter(self.download_language.as_deref(), &self.download_query))
            .unwrap_or_default()
    }
    pub fn language_options(&self) -> Vec<(Option<String>, String, usize)> {
        let Some(catalog) = &self.catalog else {
            return vec![];
        };
        let q = self.language_query.trim().to_lowercase();
        let mut out = Vec::new();
        if q.is_empty() {
            out.push((
                None,
                "All languages".to_string(),
                catalog.translations().len(),
            ));
        }
        for lang in catalog.languages() {
            if !q.is_empty()
                && ![
                    lang.name.as_str(),
                    lang.native_name.as_str(),
                    lang.code.as_str(),
                ]
                .iter()
                .any(|v| v.to_lowercase().contains(&q))
            {
                continue;
            }
            let label = if lang.native_name.is_empty()
                || lang.native_name.eq_ignore_ascii_case(&lang.name)
            {
                lang.name
            } else {
                format!("{} · {}", lang.name, lang.native_name)
            };
            out.push((Some(lang.code), label, lang.count));
        }
        out
    }

    fn screen_item_count(&self) -> usize {
        match self.screen {
            Screen::Themes => self.themes.themes().len(),
            Screen::Translations => self.filtered_translations().len(),
            Screen::Downloads => self.filtered_remote().len(),
            Screen::Languages => self.language_options().len(),
            Screen::Study => self.bookmarks.len(),
            Screen::Help | Screen::Reader => 0,
        }
    }
    fn screen_selected(&self) -> usize {
        match self.screen {
            Screen::Themes => self.theme_selected,
            Screen::Translations => self.translation_selected,
            Screen::Downloads => self.download_selected,
            Screen::Languages => self.language_selected,
            Screen::Study => self.study_selected,
            _ => 0,
        }
    }
    fn set_screen_selected(&mut self, v: usize) {
        let count = self.screen_item_count();
        let v = if count == 0 { 0 } else { v.min(count - 1) };
        match self.screen {
            Screen::Themes => self.theme_selected = v,
            Screen::Translations => self.translation_selected = v,
            Screen::Downloads => self.download_selected = v,
            Screen::Languages => self.language_selected = v,
            Screen::Study => self.study_selected = v,
            _ => {}
        }
        self.screen_scroll.scroll_to_item(v);
    }
    fn move_screen_selection(&mut self, delta: isize) {
        let c = self.screen_item_count();
        if c == 0 {
            return;
        }
        let next = step(self.screen_selected(), delta, c);
        self.set_screen_selected(next);
    }

    pub fn activate_screen_item(&mut self, cx: &mut Context<Self>) {
        match self.screen {
            Screen::Themes => {
                if self.themes.is_on_omarchy() {
                    self.status =
                        format!("Omarchy controls the theme · {}", self.themes.active().name)
                            .into();
                    self.screen = self.theme_return;
                    cx.notify();
                    return;
                }
                let i = self
                    .theme_selected
                    .min(self.themes.themes().len().saturating_sub(1));
                if self.themes.select(i) {
                    self.settings.theme = Some(self.themes.active().slug.clone());
                    let _ = self.settings.save();
                    self.apply_current_theme(cx);
                    self.status = format!("Theme: {}", self.themes.active().name).into();
                }
                self.screen = self.theme_return;
            }
            Screen::Translations => {
                let abbr = self
                    .filtered_translations()
                    .get(self.translation_selected)
                    .map(|t| t.abbreviation.clone());
                if let Some(abbr) = abbr
                    && let Some(i) = self
                        .library
                        .translations()
                        .iter()
                        .position(|t| t.abbreviation == abbr)
                {
                    self.translation_index = i;
                    self.translation_selected = i;
                    self.book_index = 0;
                    self.chapter_index = 0;
                    self.verse_index = 0;
                    self.clear_search();
                    self.save_position();
                    self.status = format!(
                        "Reading {}",
                        self.translation()
                            .map(|t| t.name.as_str())
                            .unwrap_or("translation")
                    )
                    .into();
                }
                self.screen = self.translation_return;
            }
            Screen::Downloads => self.start_selected_download(cx),
            Screen::Languages => {
                let opts = self.language_options();
                if let Some((code, _, _)) = opts.get(self.language_selected) {
                    self.download_language = code.clone();
                    self.settings.download_language = code.clone();
                    let _ = self.settings.save();
                    self.download_selected = 0;
                    self.screen = Screen::Downloads;
                }
            }
            Screen::Study => self.jump_to_selected_bookmark(cx),
            Screen::Help => self.screen = Screen::Reader,
            Screen::Reader => {
                self.clear_visual();
                self.focus = self.focus.next();
            }
        }
        cx.notify();
    }

    // --- search/bookmarks ---
    pub fn update_search(&mut self, cx: &mut Context<Self>) {
        let q = self.search_query.trim().to_lowercase();
        self.search_matches = if q.is_empty() {
            vec![]
        } else {
            self.chapter()
                .map(|c| {
                    c.verses
                        .iter()
                        .enumerate()
                        .filter(|(_, v)| v.to_lowercase().contains(&q))
                        .map(|(i, _)| i)
                        .collect()
                })
                .unwrap_or_default()
        };
        self.search_index = 0;
        if let Some(&i) = self.search_matches.first() {
            self.verse_index = i;
            self.focus = FocusPane::Verses;
        }
        self.status = if self.search_query.trim().is_empty() {
            "Search cleared".into()
        } else if self.search_matches.is_empty() {
            "No matches in this chapter".into()
        } else {
            format!(
                "{} match(es) for \"{}\"",
                self.search_matches.len(),
                self.search_query.trim()
            )
            .into()
        };
        cx.notify();
    }
    pub fn next_match(&mut self, forward: bool, cx: &mut Context<Self>) {
        if self.search_matches.is_empty() {
            self.set_status("No active search in this chapter", cx);
            return;
        }
        let c = self.search_matches.len();
        self.search_index = if forward {
            (self.search_index + 1) % c
        } else {
            (self.search_index + c - 1) % c
        };
        self.verse_index = self.search_matches[self.search_index];
        self.focus = FocusPane::Verses;
        self.scroll_to_verse(self.verse_index);
        cx.notify();
    }

    pub fn toggle_bookmark(&mut self, cx: &mut Context<Self>) {
        let Some((t, b, ch, _)) = self.current_reference() else {
            self.set_status("No verse selected", cx);
            return;
        };
        let indices = self.action_verse_indices();
        if indices.is_empty() {
            self.set_status("No verse selected", cx);
            return;
        }
        let all_bookmarked = indices
            .iter()
            .all(|index| self.bookmarks.find(&t, &b, ch, *index as u32 + 1).is_some());
        for index in &indices {
            let verse = *index as u32 + 1;
            let exists = self.bookmarks.find(&t, &b, ch, verse).is_some();
            if all_bookmarked == exists {
                self.bookmarks.toggle(&t, &b, ch, verse);
            }
        }
        let _ = self.bookmarks.save();
        let count = indices.len();
        self.set_status(
            if all_bookmarked {
                format!(
                    "Removed {count} bookmark{}",
                    if count == 1 { "" } else { "s" }
                )
            } else {
                format!(
                    "Bookmarked {count} verse{}",
                    if count == 1 { "" } else { "s" }
                )
            },
            cx,
        );
        self.clear_visual();
    }

    /// Copy the selected verse or visual range to the system clipboard.
    pub fn copy_current_verse(&mut self, cx: &mut Context<Self>) {
        let Some((_, book, chapter_number, _)) = self.current_reference() else {
            self.set_status("No verse selected", cx);
            return;
        };
        let indices = self.action_verse_indices();
        let Some(chapter) = self.chapter() else {
            self.set_status("No verse selected", cx);
            return;
        };
        let text = indices
            .iter()
            .filter_map(|index| chapter.verses.get(*index))
            .map(|text| text.trim())
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        if text.is_empty() {
            self.set_status(format!("{book} {chapter_number} has no text"), cx);
            return;
        }
        let count = indices.len();
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.set_status(
            if count == 1 {
                format!("Copied {book} {chapter_number}:{}", indices[0] + 1)
            } else {
                format!("Copied {count} verses from {book} {chapter_number}")
            },
            cx,
        );
        self.clear_visual();
    }

    fn cycle_dock(&mut self, cx: &mut Context<Self>) {
        self.settings.dock = self.settings.dock.next();
        let _ = self.settings.save();
        self.set_status(format!("Shortcut dock: {}", self.settings.dock.label()), cx);
    }

    fn settings_row_count(&self) -> usize {
        8
    }

    /// Whether a tool's state is "on" for the selected verse.
    pub fn tool_active(&self, tool: Tool) -> bool {
        let Some((translation, book, chapter, verse)) = self.current_reference() else {
            return false;
        };
        match tool {
            Tool::Bookmark => self
                .bookmarks
                .find(&translation, &book, chapter, verse)
                .is_some(),
            Tool::Note => self
                .bookmarks
                .find(&translation, &book, chapter, verse)
                .is_some_and(|bookmark| !bookmark.note.trim().is_empty()),
            Tool::Highlight => self
                .highlights
                .find(&translation, &book, chapter, verse)
                .is_some(),
            Tool::Settings => self.overlay == Overlay::Settings,
            Tool::Copy | Tool::Study => false,
        }
    }

    pub fn run_tool(&mut self, tool: Tool, window: &mut Window, cx: &mut Context<Self>) {
        self.overlay = Overlay::None;
        self.wheel_hover = None;
        match tool {
            Tool::Bookmark => self.toggle_bookmark(cx),
            Tool::Note => self.open_note_for_current_smart(window, cx),
            Tool::Highlight => self.open_highlight(cx),
            Tool::Copy => self.copy_current_verse(cx),
            Tool::Study => self.open_study(cx),
            Tool::Settings => self.open_settings_sheet(cx),
        }
        cx.notify();
    }

    /// Open the action wheel around `center`, or in the middle of the window.
    pub fn open_wheel(&mut self, center: Option<Point<Pixels>>, cx: &mut Context<Self>) {
        if self.screen != Screen::Reader || self.overlay != Overlay::None {
            return;
        }
        self.wheel_center = center;
        self.wheel_hover = None;
        self.overlay = Overlay::Wheel;
        cx.notify();
    }

    /// Notes attached to verses in the open chapter, in verse order.
    pub fn chapter_notes(&self) -> Vec<(usize, String)> {
        let (Some(translation), Some(book), Some(chapter)) =
            (self.translation(), self.book(), self.chapter())
        else {
            return Vec::new();
        };
        let mut notes: Vec<(usize, String)> = self
            .bookmarks
            .items()
            .iter()
            .filter(|bookmark| {
                bookmark.chapter == chapter.number
                    && bookmark.verse >= 1
                    && bookmark.book == book.name
                    && bookmark
                        .translation
                        .eq_ignore_ascii_case(&translation.abbreviation)
            })
            .filter_map(|bookmark| {
                let note = bookmark.note.trim();
                (!note.is_empty()).then(|| (bookmark.verse as usize - 1, note.to_string()))
            })
            .collect();
        notes.sort_by_key(|(verse, _)| *verse);
        notes
    }

    /// Select a verse from the notes stack and open its note.
    pub fn open_chapter_note(&mut self, verse_index: usize, cx: &mut Context<Self>) {
        self.verse_index = verse_index;
        self.scroll_to_verse(verse_index);
        self.notes_expanded = false;
        if let Some((translation, book, chapter, verse)) = self.current_reference() {
            self.open_note_detail(
                NoteTarget {
                    translation,
                    book,
                    chapter,
                    verse,
                },
                cx,
            );
        }
    }

    pub fn open_highlight(&mut self, cx: &mut Context<Self>) {
        let Some((translation, book, chapter, verse)) = self.current_reference() else {
            self.set_status("No verse selected", cx);
            return;
        };
        self.highlight_selected = self
            .highlights
            .find(&translation, &book, chapter, verse)
            .and_then(|highlight| {
                HighlightColor::ALL
                    .iter()
                    .position(|color| *color == highlight.color)
            })
            .unwrap_or(0);
        self.overlay = Overlay::Highlight;
        cx.notify();
    }

    fn choose_highlight(&mut self, index: usize, cx: &mut Context<Self>) {
        self.highlight_selected = index;
        self.apply_highlight(cx);
    }

    fn apply_highlight(&mut self, cx: &mut Context<Self>) {
        let Some((translation, book, chapter, _)) = self.current_reference() else {
            self.overlay = Overlay::None;
            self.set_status("No verse selected", cx);
            return;
        };
        let indices = self.action_verse_indices();
        if indices.is_empty() {
            self.overlay = Overlay::None;
            self.set_status("No verse selected", cx);
            return;
        }
        let count = indices.len();
        if self.highlight_selected >= HighlightColor::ALL.len() {
            for index in &indices {
                self.highlights
                    .remove(&translation, &book, chapter, *index as u32 + 1);
            }
            let _ = self.highlights.save();
            self.status = format!(
                "Removed highlight from {count} verse{}",
                if count == 1 { "" } else { "s" }
            )
            .into();
        } else {
            let color = HighlightColor::ALL[self.highlight_selected];
            for index in &indices {
                self.highlights
                    .set(&translation, &book, chapter, *index as u32 + 1, color);
            }
            let _ = self.highlights.save();
            self.status = format!(
                "Highlighted {count} verse{} · {}",
                if count == 1 { "" } else { "s" },
                color.label()
            )
            .into();
        }
        self.overlay = Overlay::None;
        self.clear_visual();
        cx.notify();
    }

    pub fn open_note_for_current(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((translation, book, chapter, verse)) = self.current_reference() else {
            self.set_status("No verse selected", cx);
            return;
        };
        let note = self
            .bookmarks
            .find(&translation, &book, chapter, verse)
            .map(|b| b.note.clone())
            .unwrap_or_default();
        self.open_note_prompt(
            NoteTarget {
                translation,
                book: book.clone(),
                chapter,
                verse,
            },
            note,
            window,
            cx,
        );
    }

    /// Show the existing note in a calm reading overlay.
    pub fn open_note_detail(&mut self, target: NoteTarget, cx: &mut Context<Self>) {
        self.note_detail = Some(target);
        self.overlay = Overlay::NoteDetail;
        cx.notify();
    }

    /// Open the note detail when the verse has one, otherwise the editor.
    pub fn open_note_for_current_smart(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((translation, book, chapter, verse)) = self.current_reference() else {
            self.set_status("No verse selected", cx);
            return;
        };
        let note = self
            .bookmarks
            .find(&translation, &book, chapter, verse)
            .map(|b| b.note.trim().to_string())
            .unwrap_or_default();
        if note.is_empty() {
            self.open_note_for_current(window, cx);
        } else {
            self.open_note_detail(
                NoteTarget {
                    translation,
                    book,
                    chapter,
                    verse,
                },
                cx,
            );
        }
    }

    fn edit_note_detail(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(target) = self.note_detail.clone() else {
            self.close_overlay(cx);
            return;
        };
        let note = self
            .bookmarks
            .find(
                &target.translation,
                &target.book,
                target.chapter,
                target.verse,
            )
            .map(|b| b.note.clone())
            .unwrap_or_default();
        self.overlay = Overlay::None;
        self.open_note_prompt(target, note, window, cx);
    }

    fn open_note_for_selected_bookmark(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let bm = self
            .bookmarks
            .items()
            .iter()
            .rev()
            .nth(self.study_selected)
            .cloned();
        if let Some(bm) = bm {
            self.open_note_prompt(
                NoteTarget {
                    translation: bm.translation.clone(),
                    book: bm.book.clone(),
                    chapter: bm.chapter,
                    verse: bm.verse,
                },
                bm.note,
                window,
                cx,
            );
        }
    }

    fn remove_selected_bookmark(&mut self, cx: &mut Context<Self>) {
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
            self.set_status(format!("Removed {}", removed.reference()), cx);
        }
    }

    fn jump_to_selected_bookmark(&mut self, cx: &mut Context<Self>) {
        let bm = self
            .bookmarks
            .items()
            .iter()
            .rev()
            .nth(self.study_selected)
            .cloned();
        if let Some(bm) = bm {
            self.jump_to(&bm, cx);
        }
    }
    fn jump_to(&mut self, bm: &Bookmark, cx: &mut Context<Self>) {
        let Some((ti, bi, ci, vc)) = self.locate(bm) else {
            self.set_status(format!("{} is no longer available", bm.reference()), cx);
            return;
        };
        self.translation_index = ti;
        self.book_index = bi;
        self.chapter_index = ci;
        self.verse_index = (bm.verse as usize)
            .saturating_sub(1)
            .min(vc.saturating_sub(1));
        self.focus = FocusPane::Verses;
        self.screen = Screen::Reader;
        self.clear_search();
        self.clear_visual();
        self.scroll_to_verse(self.verse_index);
        self.set_status(
            format!("Jumped to {} {}", bm.translation, bm.reference()),
            cx,
        );
    }
    fn locate(&self, bm: &Bookmark) -> Option<(usize, usize, usize, usize)> {
        let ti = self
            .library
            .translations()
            .iter()
            .position(|t| t.abbreviation.eq_ignore_ascii_case(&bm.translation))?;
        let tr = &self.library.translations()[ti];
        let bi = tr
            .books
            .iter()
            .position(|b| b.name.eq_ignore_ascii_case(&bm.book))?;
        let bk = &tr.books[bi];
        let ci = bk.chapters.iter().position(|c| c.number == bm.chapter)?;
        let vc = bk.chapters[ci].verse_count();
        (vc > 0).then_some((ti, bi, ci, vc))
    }

    // --- downloads/workers ---
    pub fn fetch_catalog(&mut self, cx: &mut Context<Self>) {
        if self.catalog.is_some() || self.catalog_loading {
            return;
        }
        self.catalog_loading = true;
        self.catalog_error = None;
        let tx = self.worker_tx.clone();
        thread::spawn(move || {
            let r = RemoteCatalog::fetch().map_err(|e| e.to_string());
            let _ = tx.send(WorkerMessage::Catalog(r));
        });
        cx.notify();
    }
    pub fn refresh_catalog(&mut self, cx: &mut Context<Self>) {
        if self.catalog_loading {
            return;
        }
        self.catalog = None;
        self.catalog_error = None;
        self.fetch_catalog(cx);
    }
    fn start_selected_download(&mut self, cx: &mut Context<Self>) {
        if self.download.is_some() {
            return;
        }
        let sel = self
            .filtered_remote()
            .get(self.download_selected)
            .map(|t| (t.id.clone(), t.title().to_string()));
        let Some((id, name)) = sel else {
            self.set_status("Choose a translation first", cx);
            return;
        };
        let control = DownloadControl::new();
        self.download = Some(DownloadState {
            id: id.clone(),
            name,
            control: control.clone(),
        });
        self.status = format!("Downloading {id}...").into();
        let tx = self.worker_tx.clone();
        thread::spawn(move || {
            let r = remote::download_translation(&id, &control)
                .and_then(|tr| {
                    let sample = fonts::sample_text(&tr);
                    remote::save_download(&tr, &id)?;
                    let font_notice = fonts::prepare_for_text(&sample, true);
                    Ok(DownloadDone {
                        id: id.clone(),
                        name: tr.name,
                        font_notice,
                    })
                })
                .map_err(|e| e.to_string());
            let _ = tx.send(WorkerMessage::Download(r));
        });
        cx.notify();
    }

    pub fn poll_worker(&mut self, cx: &mut Context<Self>) {
        let mut changed = false;
        loop {
            match self.worker_rx.try_recv() {
                Ok(WorkerMessage::Catalog(r)) => {
                    self.catalog_loading = false;
                    match r {
                        Ok(c) => {
                            self.catalog = Some(c);
                            self.catalog_error = None;
                            self.download_selected = 0;
                        }
                        Err(e) => self.catalog_error = Some(e),
                    }
                    changed = true;
                }
                Ok(WorkerMessage::Download(r)) => {
                    let cancelled = self
                        .download
                        .as_ref()
                        .is_some_and(|d| d.control.is_cancelled());
                    self.download = None;
                    match r {
                        Ok(done) => {
                            self.library = Library::load_default();
                            if let Some(i) = self
                                .library
                                .translations()
                                .iter()
                                .position(|t| t.abbreviation.eq_ignore_ascii_case(&done.id))
                            {
                                self.translation_index = i;
                                self.book_index = 0;
                                self.chapter_index = 0;
                                self.verse_index = 0;
                                self.translation_selected = i;
                            }
                            self.save_position();
                            let mut msg = format!("Added {} from bible.helloao.org", done.name);
                            if let Some(n) = done.font_notice {
                                msg.push_str(" · ");
                                msg.push_str(&n);
                            }
                            self.status = msg.into();
                            self.screen = Screen::Reader;
                        }
                        Err(e) => {
                            self.status = if cancelled {
                                "Download cancelled".into()
                            } else {
                                format!("Download failed: {e}").into()
                            }
                        }
                    }
                    changed = true;
                }
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            }
        }
        if changed {
            cx.notify();
        }
    }

    // --- input prompt (gpui-kit Input) ---
    fn create_prompt(
        &mut self,
        kind: InputKind,
        label: impl Into<SharedString>,
        value: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let label = label.into();
        let state = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(value)
                .placeholder(label.clone())
        });
        let state_for_sub = state.clone();
        cx.subscribe_in(
            &state_for_sub,
            window,
            move |this, _state, event, window, cx| {
                match event {
                    InputEvent::PressEnter { .. } => this.commit_input(window, cx),
                    InputEvent::Blur => { /* keep prompt open; commit/cancel is explicit */ }
                    _ => {}
                }
            },
        )
        .detach();
        let focus = state.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
        self.input = Some(InputPrompt { kind, state });
        cx.notify();
    }

    pub fn open_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.create_prompt(
            InputKind::Search,
            "Search this chapter",
            self.search_query.clone(),
            window,
            cx,
        );
    }
    pub fn open_filter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.screen {
            Screen::Translations => self.create_prompt(
                InputKind::TranslationFilter,
                "Filter translations",
                self.translation_query.clone(),
                window,
                cx,
            ),
            Screen::Downloads => self.create_prompt(
                InputKind::DownloadFilter,
                "Search translations",
                self.download_query.clone(),
                window,
                cx,
            ),
            Screen::Languages => self.create_prompt(
                InputKind::LanguageFilter,
                "Search languages",
                self.language_query.clone(),
                window,
                cx,
            ),
            _ => {}
        }
    }
    pub fn open_note_prompt(
        &mut self,
        target: NoteTarget,
        note: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.note_target = Some(target.clone());
        self.create_prompt(
            InputKind::Note,
            format!("Note · {} {}:{}", target.book, target.chapter, target.verse),
            note,
            window,
            cx,
        );
    }

    pub fn commit_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(prompt) = self.input.take() else {
            return;
        };
        let value = prompt.state.read(cx).value().to_string();
        window.focus(&self.focus_handle, cx);
        match prompt.kind {
            InputKind::Search => {
                self.search_query = value.trim().to_string();
                self.update_search(cx);
                cx.notify();
                return;
            }
            InputKind::TranslationFilter => {
                self.translation_query = value.trim().to_string();
                self.translation_selected = 0;
            }
            InputKind::DownloadFilter => {
                self.download_query = value.trim().to_string();
                self.download_selected = 0;
            }
            InputKind::LanguageFilter => {
                self.language_query = value.trim().to_string();
                self.language_selected = 0;
            }
            InputKind::Note => {
                if let Some(target) = self.note_target.take() {
                    self.bookmarks.set_note(
                        &target.translation,
                        &target.book,
                        target.chapter,
                        target.verse,
                        &value,
                    );
                    let _ = self.bookmarks.save();
                    self.status = format!(
                        "Saved note for {} {}:{}",
                        target.book, target.chapter, target.verse
                    )
                    .into();
                }
            }
        }
        cx.notify();
    }
    pub fn cancel_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.input = None;
        self.note_target = None;
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    // --- key handling fallback (direct) ---
    pub fn handle_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ks = &event.keystroke;
        let key = ks.key.as_str();
        let ctrl = ks.modifiers.control;
        let shift = ks.modifiers.shift;

        if self.input.is_some() {
            match key {
                "escape" => self.cancel_input(window, cx),
                "enter" => {
                    /* handled by InputEvent::PressEnter subscription when focused; fallback */
                    self.commit_input(window, cx);
                }
                _ => {} // let focused Input component handle text editing
            }
            if key == "escape" || key == "enter" {
                return;
            }
            // For other keys, allow propagation/handling by focused elements where possible;
            // but root handler may still run. We avoid custom char insertion.
            if !ctrl {
                return;
            }
        }

        if self.overlay != Overlay::None {
            self.handle_overlay_key(key, ctrl, shift, window, cx);
            return;
        }

        if ctrl && key == "c" {
            // Copy a mouse selection when one exists; Ctrl+C is still the
            // quit shortcut when nothing is selected.
            let selected = TextSelection::selected_text(window, cx);
            if selected.trim().is_empty() {
                cx.quit();
            } else {
                cx.write_to_clipboard(ClipboardItem::new_string(selected));
                self.set_status("Copied selection", cx);
            }
            return;
        }
        if ctrl && shift && key == "d" {
            self.cycle_dock(cx);
            return;
        }
        if ctrl && key == "d" {
            self.page(1, cx);
            return;
        }
        if ctrl && key == "u" {
            self.page(-1, cx);
            return;
        }

        if key == "g" && !shift && !ctrl {
            if self.pending_g {
                self.pending_g = false;
                self.go_top(cx);
            } else {
                self.pending_g = true;
            }
            return;
        }
        self.pending_g = false;

        // Vim-style visual mode: movement extends the range and the usual
        // yank/bookmark/highlight keys act on the whole selection.
        if self.visual_anchor.is_some() {
            match key {
                "escape" => {
                    self.visual_anchor = None;
                    self.set_status("Visual off", cx);
                    return;
                }
                "v" | "V" => {
                    self.toggle_visual(cx);
                    return;
                }
                "j" | "down" => {
                    self.move_down(cx);
                    return;
                }
                "k" | "up" => {
                    self.move_up(cx);
                    return;
                }
                "y" if !ctrl => {
                    self.copy_current_verse(cx);
                    return;
                }
                "m" if !shift => {
                    self.toggle_bookmark(cx);
                    return;
                }
                "h" if shift => {
                    self.open_highlight(cx);
                    return;
                }
                "M" if !ctrl => {
                    self.open_note_for_current(window, cx);
                    return;
                }
                _ => {
                    // Any other action leaves visual mode and runs normally.
                    self.visual_anchor = None;
                }
            }
        }

        match key {
            "q" if self.screen == Screen::Reader => {
                self.save_position();
                cx.quit();
            }
            "q" | "escape" => {
                if self.screen == Screen::Downloads && self.download.is_some() && key == "escape" {
                    if let Some(d) = &self.download {
                        d.control.cancel();
                    }
                    self.status = "Cancelling download…".into();
                    cx.notify();
                } else if self.screen == Screen::Reader {
                    self.clear_search();
                    cx.notify();
                } else {
                    self.close_screen(cx);
                }
            }
            "j" | "down" => self.move_down(cx),
            "k" | "up" => self.move_up(cx),
            "h" if shift && self.screen == Screen::Reader => self.open_highlight(cx),
            "h" | "left" if !shift => self.move_left(cx),
            "right" | "tab" => self.move_right(cx),
            "l" if !shift => {
                if self.screen == Screen::Downloads {
                    self.language_query.clear();
                    self.language_selected = 0;
                    self.screen = Screen::Languages;
                    cx.notify();
                } else {
                    self.move_right(cx);
                }
            }
            "[" => {
                if self.screen == Screen::Reader {
                    self.prev_chapter(cx);
                }
            }
            "]" => {
                if self.screen == Screen::Reader {
                    self.next_chapter(cx);
                }
            }
            "pagedown" => self.page(1, cx),
            "pageup" => self.page(-1, cx),
            "home" => self.go_top(cx),
            "end" | "G" => self.go_bottom(cx),
            "g" if shift => self.go_bottom(cx),
            "enter" => {
                if self.screen == Screen::Reader {
                    self.open_navigator(cx);
                } else {
                    self.activate_screen_item(cx);
                }
            }
            "v" | "V" if self.screen == Screen::Reader && !ctrl => self.toggle_visual(cx),
            "space" | " " if self.screen == Screen::Reader && !ctrl => self.open_wheel(None, cx),
            "/" if !shift => {
                if self.screen == Screen::Reader {
                    self.open_search(window, cx);
                } else {
                    self.open_filter(window, cx);
                }
            }
            "/" | "?" => self.open_help(cx),
            "n" if self.screen == Screen::Reader && !shift => self.next_match(true, cx),
            "n" | "N" if self.screen == Screen::Reader && shift => self.next_match(false, cx),
            "y" if self.screen == Screen::Reader && !shift && !ctrl => self.copy_current_verse(cx),
            "m" if self.screen == Screen::Reader && !shift => self.toggle_bookmark(cx),
            "m" | "M" if self.screen == Screen::Reader && shift => {
                self.open_note_for_current(window, cx)
            }
            "M" if self.screen == Screen::Reader => self.open_note_for_current(window, cx),
            "o" if self.screen == Screen::Reader => self.open_navigator(cx),
            "t" if shift => self.open_themes(self.screen, cx),
            "T" => self.open_themes(self.screen, cx),
            "t" => self.open_translations(self.screen, cx),
            "d" if shift => self.open_downloads(self.screen, cx),
            "D" => self.open_downloads(self.screen, cx),
            "s" if shift => self.open_study(cx),
            "S" => self.open_study(cx),
            "," => self.open_settings_sheet(cx),
            "M" if self.screen == Screen::Study => self.open_note_for_selected_bookmark(window, cx),
            "x" if self.screen == Screen::Study => self.remove_selected_bookmark(cx),
            "r" if self.screen == Screen::Downloads => self.refresh_catalog(cx),
            _ => {}
        }
    }

    // --- rendering helpers ---
    fn screen_title(&self) -> &'static str {
        match self.screen {
            Screen::Reader => "Reader",
            Screen::Translations => "Translations",
            Screen::Themes => "Themes",
            Screen::Downloads => "Downloads",
            Screen::Languages => "Languages",
            Screen::Study => "Study & Bookmarks",
            Screen::Help => "Help",
        }
    }
}

fn step(index: usize, delta: isize, count: usize) -> usize {
    if count == 0 {
        return 0;
    }
    index.saturating_add_signed(delta).min(count - 1)
}

impl Render for OmaBibleApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.poll_worker(cx);
        self.clamp_indices();

        let body: AnyElement = match self.screen {
            Screen::Reader => IntoElement::into_any_element(ReaderView::new(cx.entity())),
            _ => render_simple_screen(self, cx).into_any_element(),
        };

        let input_bar = self.input.as_ref().map(|prompt| {
            h_flex()
                .w_full()
                .justify_center()
                .px(px(24.))
                .pb(px(10.))
                .child(
                    h_flex()
                        .w_full()
                        .max_w(px(720.))
                        .items_center()
                        .gap(px(10.))
                        .px(px(14.))
                        .py(px(10.))
                        .rounded(px(12.))
                        .bg(cx.theme().popover)
                        .border_1()
                        .border_color(cx.theme().border)
                        .shadow_md()
                        .child(
                            Icon::new(match prompt.kind {
                                InputKind::Note => IconName::StickyNote,
                                _ => IconName::Search,
                            })
                            .size(px(16.))
                            .text_color(cx.theme().muted_foreground),
                        )
                        .child(
                            div()
                                .flex_1()
                                .child(Input::new(&prompt.state).cleanable(true)),
                        )
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(cx.theme().muted_foreground)
                                .child("Enter · Esc"),
                        ),
                )
        });

        let status_bar = render_status_bar(self, cx);
        let overlay = render_overlay(self, cx);
        let on_reader = self.screen == Screen::Reader;
        let dock = on_reader.then(|| chrome::render_dock(self, cx)).flatten();
        let notes = on_reader
            .then(|| chrome::render_notes_stack(self, cx))
            .flatten();

        let content = v_flex()
            .relative()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(
                v_flex()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(div().flex_1().min_h_0().child(body))
                    .children(input_bar)
                    .children(notes)
                    .children(dock),
            )
            .child(status_bar);

        div()
            .id("oma-bible-root")
            .key_context("OmaBible")
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::handle_key_down))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    let viewport = window.viewport_size();
                    let center = chrome::clamp_wheel_center(event.position, viewport);
                    this.open_wheel(Some(center), cx);
                }),
            )
            .relative()
            .size_full()
            .child(content)
            .children(overlay)
    }
}

/// Keep the page scrolling when the pointer is over the chrome instead of
/// the reading column.
fn scroll_reading_by(
    app: &mut OmaBibleApp,
    event: &ScrollEvent,
    window: &mut Window,
    cx: &mut Context<OmaBibleApp>,
) {
    let delta = event.delta.pixel_delta(window.line_height());
    let offset = app.reading_scroll.offset();
    let max = app.reading_scroll.max_offset();
    let next = (offset.y + delta.y).clamp(-max.y, px(0.));
    app.reading_scroll.set_offset(point(offset.x, next));
    cx.notify();
}

fn render_status_bar(app: &OmaBibleApp, cx: &mut Context<OmaBibleApp>) -> AnyElement {
    let word_count: usize = app
        .chapter()
        .map(|chapter| {
            chapter
                .verses
                .iter()
                .map(|verse| verse.split_whitespace().count())
                .sum()
        })
        .unwrap_or(0);
    let visual_count = app.visual_range().map(|_| app.visual_verse_count());
    let status = if let Some(count) = visual_count {
        Some(
            format!(
                "VISUAL · {count} verse{}",
                if count == 1 { "" } else { "s" }
            )
            .into(),
        )
    } else {
        (app.status.as_ref() != "Ready").then_some(app.status.clone())
    };

    h_flex()
        .w_full()
        .items_center()
        .px(px(18.))
        .py(px(5.))
        .on_scroll(cx.listener(scroll_reading_by))
        .child(div().flex_1())
        .child(
            div()
                .flex_1()
                .text_center()
                .text_size(px(11.))
                .text_color(if visual_count.is_some() {
                    cx.theme().accent
                } else {
                    cx.theme().muted_foreground
                })
                .children(status),
        )
        .child(
            div()
                .flex_1()
                .text_right()
                .text_size(px(11.))
                .text_color(cx.theme().muted_foreground)
                .child(format!("{word_count} words")),
        )
        .into_any_element()
}

fn render_overlay(app: &OmaBibleApp, cx: &mut Context<OmaBibleApp>) -> Option<AnyElement> {
    match app.overlay {
        Overlay::None => None,
        Overlay::Navigator => Some(render_navigator(app, cx)),
        Overlay::Settings => Some(render_settings_sheet(app, cx)),
        Overlay::Highlight => Some(render_highlight_sheet(app, cx)),
        Overlay::NoteDetail => Some(render_note_detail(app, cx)),
        Overlay::Wheel => Some(chrome::render_wheel(app, cx)),
    }
}

/// Render a saved note as a calm card: date and reference on top, then the
/// note itself. The inline `Note · …` line in the reader stays in place and
/// opens this view when clicked.
fn render_note_detail(app: &OmaBibleApp, cx: &mut Context<OmaBibleApp>) -> AnyElement {
    let Some(target) = app.note_detail.as_ref() else {
        return overlay_shell(
            cx,
            540.,
            div()
                .p(px(24.))
                .text_size(px(15.))
                .child("This note is no longer available."),
        );
    };

    let bookmark = app.bookmarks.find(
        &target.translation,
        &target.book,
        target.chapter,
        target.verse,
    );
    let note = bookmark
        .map(|bookmark| bookmark.note.trim())
        .filter(|note| !note.is_empty())
        .unwrap_or("No note yet.");
    let date = bookmark
        .map(|bookmark| format_note_date(bookmark.created))
        .unwrap_or_default();
    let reference = format!("{} {}:{}", target.book, target.chapter, target.verse);

    let card = v_flex()
        .w_full()
        .max_w(px(600.))
        .rounded(px(18.))
        .bg(cx.theme().popover)
        .border_1()
        .border_color(cx.theme().border)
        .shadow_xl()
        .overflow_hidden()
        .child(
            h_flex()
                .w_full()
                .items_start()
                .justify_between()
                .px(px(24.))
                .pt(px(20.))
                .pb(px(12.))
                .child(
                    v_flex()
                        .gap(px(3.))
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(cx.theme().muted_foreground)
                                .child("NOTE"),
                        )
                        .child(div().text_size(px(18.)).child(reference)),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(cx.theme().muted_foreground)
                        .child(date),
                ),
        )
        .child(
            div()
                .id("note-detail-body")
                .w_full()
                .max_h(px(380.))
                .overflow_y_scroll()
                .px(px(24.))
                .pb(px(18.))
                .text_size(px(16.))
                .line_height(px(26.))
                .child(note.to_string()),
        )
        .child(
            h_flex()
                .w_full()
                .items_center()
                .justify_between()
                .px(px(24.))
                .py(px(12.))
                .border_t_1()
                .border_color(cx.theme().border)
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(cx.theme().muted_foreground)
                        .child("e edit · Esc close"),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(cx.theme().muted_foreground)
                        .child(target.translation.to_uppercase()),
                ),
        );

    overlay_shell(cx, 600., card)
}

/// Civil date from a Unix timestamp, without pulling in a date crate:
/// `9 Nov 2026`.
pub fn format_note_date(seconds: u64) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let days = (seconds / 86_400) as i64;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    let year = year + if month <= 2 { 1 } else { 0 };
    format!("{day} {} {year}", MONTHS[(month - 1) as usize])
}

fn overlay_shell(
    cx: &mut Context<OmaBibleApp>,
    max_width: f32,
    card: impl IntoElement,
) -> AnyElement {
    div()
        .id("overlay-scrim")
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .bg(rgba(0x00000066))
        .on_click(cx.listener(|this, _, _, cx| this.close_overlay(cx)))
        .child(
            div()
                .id("overlay-card")
                .w_full()
                .max_w(px(max_width))
                .px(px(20.))
                .on_click(|_, _, cx| cx.stop_propagation())
                .child(card),
        )
        .into_any_element()
}

fn render_navigator(app: &OmaBibleApp, cx: &mut Context<OmaBibleApp>) -> AnyElement {
    let books = app
        .translation()
        .map(|translation| {
            translation
                .books
                .iter()
                .map(|book| book.name.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let chapters = app
        .nav_book()
        .map(|book| {
            book.chapters
                .iter()
                .map(|chapter| chapter.number)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let book_name = books
        .get(app.nav_book_index)
        .cloned()
        .unwrap_or_else(|| "Choose a book".into());
    let translation_name = app
        .translation()
        .map(|translation| translation.name.clone())
        .unwrap_or_else(|| "No translation".into());

    let card = v_flex()
        .w_full()
        .max_w(px(780.))
        // A definite height lets the book and chapter panes become the
        // scrollable region; `max_h` alone lets their content overflow the
        // card and get clipped instead of scrolling.
        .h(px(620.))
        .rounded(px(18.))
        .bg(cx.theme().popover)
        .border_1()
        .border_color(cx.theme().border)
        .shadow_xl()
        .overflow_hidden()
        .child(
            h_flex()
                .w_full()
                .items_center()
                .justify_between()
                .px(px(24.))
                .py(px(18.))
                .border_b_1()
                .border_color(cx.theme().border)
                .child(
                    v_flex()
                        .gap(px(2.))
                        .child(div().text_size(px(18.)).child("Book & chapter"))
                        .child(
                            div()
                                .text_size(px(12.))
                                .text_color(cx.theme().muted_foreground)
                                .child(format!("{translation_name} · {book_name}")),
                        ),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(cx.theme().muted_foreground)
                        .child("Esc"),
                ),
        )
        .child(
            h_flex()
                .w_full()
                .flex_1()
                .min_h_0()
                .items_stretch()
                .overflow_hidden()
                .child(
                    v_flex()
                        .id("navigator-books")
                        .w(px(272.))
                        .flex_shrink_0()
                        .min_h_0()
                        .overflow_y_scroll()
                        .track_scroll(&app.nav_book_scroll)
                        .p(px(8.))
                        .gap(px(2.))
                        .children(books.into_iter().enumerate().map(|(index, name)| {
                            let selected = index == app.nav_book_index;
                            let mut row = div()
                                .id(SharedString::from(format!("navigator-book-{index}")))
                                .w_full()
                                .px(px(12.))
                                .py(px(7.))
                                .rounded(px(8.))
                                .text_size(px(14.))
                                .cursor_pointer()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.nav_select_book(index, cx);
                                    cx.notify();
                                }))
                                .child(name);
                            if selected {
                                row = row
                                    .bg(cx.theme().selection)
                                    .text_color(cx.theme().foreground);
                            } else {
                                row = row
                                    .text_color(cx.theme().muted_foreground)
                                    .hover(|style| style.bg(cx.theme().muted));
                            }
                            row
                        })),
                )
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .min_h_0()
                        .child(
                            h_flex()
                                .w_full()
                                .items_center()
                                .justify_between()
                                .px(px(22.))
                                .pt(px(18.))
                                .pb(px(10.))
                                .child(
                                    div()
                                        .text_size(px(11.))
                                        .text_color(cx.theme().muted_foreground)
                                        .child("CHAPTER"),
                                )
                                .child(
                                    div()
                                        .text_size(px(11.))
                                        .text_color(cx.theme().muted_foreground)
                                        .child(format!("{} chapters", chapters.len())),
                                ),
                        )
                        // A row-wrapping box lays the chapters out as a grid
                        // and keeps each button a direct child of the scroll
                        // container, so `scroll_to_item` can reveal it.
                        .child(
                            h_flex()
                                .id("navigator-chapters")
                                .w_full()
                                .flex_1()
                                .min_h_0()
                                .items_start()
                                .flex_wrap()
                                .content_start()
                                .overflow_y_scroll()
                                .track_scroll(&app.nav_chapter_scroll)
                                .px(px(22.))
                                .pb(px(22.))
                                .gap(px(8.))
                                .children(chapters.into_iter().enumerate().map(
                                    |(index, number)| {
                                        let selected = index == app.nav_chapter_index;
                                        let mut chapter = div()
                                            .id(SharedString::from(format!(
                                                "navigator-chapter-{index}"
                                            )))
                                            .w(px(48.))
                                            .h(px(42.))
                                            .flex_shrink_0()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .rounded(px(9.))
                                            .text_size(px(14.))
                                            .cursor_pointer()
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.nav_chapter_index = index;
                                                this.apply_navigator(cx);
                                            }))
                                            .child(number.to_string());
                                        if selected {
                                            chapter = chapter
                                                .bg(cx.theme().accent)
                                                .text_color(cx.theme().accent_foreground);
                                        } else {
                                            chapter = chapter
                                                .bg(cx.theme().secondary)
                                                .text_color(cx.theme().foreground)
                                                .hover(|style| style.bg(cx.theme().selection));
                                        }
                                        chapter
                                    },
                                )),
                        ),
                ),
        )
        .child(
            h_flex()
                .w_full()
                .items_center()
                .justify_between()
                .px(px(24.))
                .py(px(12.))
                .border_t_1()
                .border_color(cx.theme().border)
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(cx.theme().muted_foreground)
                        .child("o opens this sheet"),
                ),
        );

    overlay_shell(cx, 780., card)
}

fn render_settings_sheet(app: &OmaBibleApp, cx: &mut Context<OmaBibleApp>) -> AnyElement {
    let theme_label = if app.themes.is_on_omarchy() {
        format!("{} · Omarchy", app.themes.active().name)
    } else {
        app.themes.active().name.clone()
    };
    let translation = app
        .translation()
        .map(|translation| format!("{} ({})", translation.name, translation.abbreviation))
        .unwrap_or_else(|| "None".into());
    let rows: Vec<(IconName, &'static str, String, bool)> = vec![
        (IconName::Palette, "Theme", theme_label, true),
        (IconName::Languages, "Translation", translation, true),
        (
            IconName::CloudDownload,
            "Downloads",
            "Free Use Bible API".into(),
            true,
        ),
        (
            IconName::BookMarked,
            "Study & bookmarks",
            format!("{} saved", app.bookmarks.len()),
            true,
        ),
        (
            IconName::Hash,
            "Verse numbers",
            if app.settings.show_verse_numbers {
                "On".into()
            } else {
                "Off".into()
            },
            false,
        ),
        (
            IconName::ALargeSmall,
            "Text size",
            format!("{:.0} px", app.reader_font_size()),
            false,
        ),
        (
            IconName::PanelBottom,
            "Dock",
            app.settings.dock.label().into(),
            false,
        ),
        (IconName::CircleQuestionMark, "Help", String::new(), true),
    ];

    let card = v_flex()
        .w_full()
        .max_w(px(560.))
        .rounded(px(18.))
        .bg(cx.theme().popover)
        .border_1()
        .border_color(cx.theme().border)
        .shadow_xl()
        .overflow_hidden()
        .child(
            h_flex()
                .w_full()
                .items_center()
                .justify_between()
                .px(px(24.))
                .py(px(18.))
                .border_b_1()
                .border_color(cx.theme().border)
                .child(
                    v_flex()
                        .gap(px(2.))
                        .child(div().text_size(px(18.)).child("Reading settings"))
                        .child(
                            div()
                                .text_size(px(12.))
                                .text_color(cx.theme().muted_foreground)
                                .child(if app.themes.is_on_omarchy() {
                                    "Theme follows Omarchy"
                                } else {
                                    "Theme, library and reading comfort"
                                }),
                        ),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(cx.theme().muted_foreground)
                        .child("Esc"),
                ),
        )
        .child(
            v_flex()
                .w_full()
                .p(px(8.))
                .children(rows.into_iter().enumerate().map(
                    |(index, (icon, label, value, chevron))| {
                        let selected = index == app.settings_selected;
                        let mut row = h_flex()
                            .id(SharedString::from(format!("settings-row-{index}")))
                            .w_full()
                            .items_center()
                            .gap(px(12.))
                            .px(px(14.))
                            .py(px(12.))
                            .rounded(px(10.))
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.settings_selected = index;
                                this.activate_settings_sheet_item(cx);
                            }))
                            .child(
                                Icon::new(icon)
                                    .size(px(16.))
                                    .text_color(cx.theme().muted_foreground),
                            )
                            .child(div().flex_1().text_size(px(14.)).child(label))
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(cx.theme().muted_foreground)
                                    .child(value),
                            )
                            .when(chevron, |this| {
                                this.child(
                                    Icon::new(IconName::ChevronRight)
                                        .size(px(14.))
                                        .text_color(cx.theme().muted_foreground),
                                )
                            });
                        if selected {
                            row = row.bg(cx.theme().selection);
                        } else {
                            row = row.hover(|style| style.bg(cx.theme().muted));
                        }
                        row
                    },
                )),
        )
        .child(
            h_flex()
                .w_full()
                .items_center()
                .justify_between()
                .px(px(24.))
                .py(px(12.))
                .border_t_1()
                .border_color(cx.theme().border)
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(cx.theme().muted_foreground)
                        .child("j/k move · Enter choose · +/- text size"),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(cx.theme().muted_foreground)
                        .child(", opens settings"),
                ),
        );

    overlay_shell(cx, 560., card)
}

fn render_highlight_sheet(app: &OmaBibleApp, cx: &mut Context<OmaBibleApp>) -> AnyElement {
    let reference = app
        .current_reference()
        .map(|(_, book, chapter, verse)| format!("{book} {chapter}:{verse}"))
        .unwrap_or_else(|| "No verse selected".to_string());
    let current = app
        .current_reference()
        .and_then(|(translation, book, chapter, verse)| {
            app.highlights.find(&translation, &book, chapter, verse)
        })
        .map(|highlight| highlight.color);

    let card = v_flex()
        .w_full()
        .max_w(px(440.))
        .rounded(px(18.))
        .bg(cx.theme().popover)
        .border_1()
        .border_color(cx.theme().border)
        .shadow_xl()
        .overflow_hidden()
        .child(
            h_flex()
                .w_full()
                .items_center()
                .justify_between()
                .px(px(24.))
                .py(px(18.))
                .border_b_1()
                .border_color(cx.theme().border)
                .child(
                    v_flex()
                        .gap(px(2.))
                        .child(div().text_size(px(18.)).child("Highlight"))
                        .child(
                            div()
                                .text_size(px(12.))
                                .text_color(cx.theme().muted_foreground)
                                .child(reference),
                        ),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(cx.theme().muted_foreground)
                        .child("Esc"),
                ),
        )
        .child(
            v_flex().w_full().p(px(8.)).gap(px(2.)).children(
                HighlightColor::ALL
                    .into_iter()
                    .enumerate()
                    .map(|(index, color)| {
                        let selected = index == app.highlight_selected;
                        let swatch = app_theme::highlight(color, cx);
                        let mut row = h_flex()
                            .id(SharedString::from(format!("highlight-color-{index}")))
                            .w_full()
                            .items_center()
                            .gap(px(12.))
                            .px(px(14.))
                            .py(px(10.))
                            .rounded(px(9.))
                            .cursor_pointer()
                            .on_click(
                                cx.listener(move |this, _, _, cx| this.choose_highlight(index, cx)),
                            )
                            .child(div().size(px(16.)).rounded(px(4.)).bg(swatch))
                            .child(div().flex_1().text_size(px(14.)).child(color.label()));
                        if current == Some(color) {
                            row = row.child(
                                Icon::new(IconName::Check)
                                    .size(px(14.))
                                    .text_color(cx.theme().accent),
                            );
                        }
                        if selected {
                            row = row.bg(cx.theme().selection);
                        } else {
                            row = row.hover(|style| style.bg(cx.theme().muted));
                        }
                        row
                    })
                    .chain(std::iter::once({
                        let index = HighlightColor::ALL.len();
                        let selected = index == app.highlight_selected;
                        let mut row = h_flex()
                            .id("highlight-remove")
                            .w_full()
                            .items_center()
                            .gap(px(12.))
                            .px(px(14.))
                            .py(px(10.))
                            .rounded(px(9.))
                            .cursor_pointer()
                            .on_click(
                                cx.listener(move |this, _, _, cx| this.choose_highlight(index, cx)),
                            )
                            .child(
                                Icon::new(IconName::Ban)
                                    .size(px(16.))
                                    .text_color(cx.theme().muted_foreground),
                            )
                            .child(div().flex_1().text_size(px(14.)).child("No highlight"));
                        if selected {
                            row = row.bg(cx.theme().selection);
                        } else {
                            row = row.hover(|style| style.bg(cx.theme().muted));
                        }
                        row
                    })),
            ),
        )
        .child(
            h_flex()
                .w_full()
                .items_center()
                .justify_between()
                .px(px(24.))
                .py(px(12.))
                .border_t_1()
                .border_color(cx.theme().border)
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(cx.theme().muted_foreground)
                        .child("j/k move · Enter apply · x remove"),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(cx.theme().muted_foreground)
                        .child("H opens this sheet"),
                ),
        );

    overlay_shell(cx, 440., card)
}

fn render_simple_screen(app: &OmaBibleApp, cx: &mut Context<OmaBibleApp>) -> impl IntoElement {
    let title = app.screen_title();
    let subtitle = match app.screen {
        Screen::Downloads => {
            "The Free Use Bible API catalogue. / filter · l language · r refresh · Enter download · Esc back"
        }
        Screen::Languages => {
            "Choose a language for the download catalogue. / filter · Enter apply · Esc back"
        }
        Screen::Translations => {
            "Choose an installed translation. / filter · Enter apply · Esc back"
        }
        Screen::Themes => {
            if app.themes.is_on_omarchy() {
                "Omarchy is controlling the colours. This list is read-only here. Esc back"
            } else {
                "Choose a palette for the reader. Enter apply · Esc back"
            }
        }
        Screen::Study => "Bookmarks and notes. Enter jump · M edit note · x remove · Esc back",
        Screen::Help => "Keyboard shortcuts and usage. Esc or q to close",
        Screen::Reader => "",
    };

    let items: Vec<(String, bool, bool)> = match app.screen {
        Screen::Translations => {
            let current = app.translation().map(|t| t.abbreviation.as_str());
            app.filtered_translations()
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    (
                        format!("{} ({}) · {}", t.name, t.abbreviation, t.language),
                        Some(t.abbreviation.as_str()) == current,
                        i == app.translation_selected,
                    )
                })
                .collect()
        }
        Screen::Themes => {
            let cur = app.themes.active().slug.clone();
            app.themes
                .themes()
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    (
                        format!(
                            "{}{}",
                            t.name,
                            if t.mode.is_dark() {
                                " · dark"
                            } else {
                                " · light"
                            }
                        ),
                        t.slug == cur,
                        i == app.theme_selected,
                    )
                })
                .collect()
        }
        Screen::Downloads => {
            let downloading = app.download.as_ref().map(|d| d.id.as_str());
            if let Some(err) = &app.catalog_error {
                vec![(format!("Could not load catalogue: {err}"), false, false)]
            } else if app.catalog_loading {
                vec![(
                    "Loading translations from bible.helloao.org…".to_string(),
                    false,
                    false,
                )]
            } else {
                let mut rows = vec![];
                if let Some(d) = &app.download {
                    rows.push((
                        format!(
                            "Downloading {} ({} bytes received) · Esc to cancel",
                            d.name,
                            d.control.downloaded()
                        ),
                        true,
                        false,
                    ));
                }
                rows.extend(app.filtered_remote().iter().enumerate().map(|(i, t)| {
                    (
                        format!(
                            "{} · {} · {} ({}){}",
                            t.title(),
                            t.language_label(),
                            t.english_title(),
                            t.id,
                            if downloading == Some(t.id.as_str()) {
                                " · downloading"
                            } else {
                                ""
                            }
                        ),
                        false,
                        i == app.download_selected,
                    )
                }));
                if rows.is_empty() {
                    rows.push((
                        "No translations match the current filter.".to_string(),
                        false,
                        false,
                    ));
                }
                rows
            }
        }
        Screen::Languages => app
            .language_options()
            .iter()
            .enumerate()
            .map(|(i, (code, label, count))| {
                let cur = app.download_language.as_deref() == code.as_deref();
                (
                    format!("{} ({count})", label),
                    cur,
                    i == app.language_selected,
                )
            })
            .collect(),
        Screen::Study => {
            if app.bookmarks.is_empty() {
                vec![(
                    "No bookmarks yet. Press m on a verse in the Reader to bookmark it."
                        .to_string(),
                    false,
                    false,
                )]
            } else {
                app.bookmarks
                    .items()
                    .iter()
                    .rev()
                    .enumerate()
                    .map(|(i, bm)| {
                        let note = if bm.note.trim().is_empty() {
                            String::new()
                        } else {
                            format!(" — {}", bm.note.trim())
                        };
                        (
                            format!("{} {}{}", bm.translation, bm.reference(), note),
                            false,
                            i == app.study_selected,
                        )
                    })
                    .collect()
            }
        }
        Screen::Help => vec![
            ("o · book & chapter selector".to_string(), false, false),
            (
                "j/k · previous/next verse · h/l · previous/next chapter".to_string(),
                false,
                false,
            ),
            (
                "gg/Home · first verse · G/End · last verse · Ctrl+u/d · page".to_string(),
                false,
                false,
            ),
            (
                "/ · search this chapter · n/N · next/previous match".to_string(),
                false,
                false,
            ),
            (
                "v · visual mode · y copy · m bookmark · H highlight · Esc exits".to_string(),
                false,
                false,
            ),
            (
                "m · bookmark · M · note · H · highlight · y · copy · S · study".to_string(),
                false,
                false,
            ),
            (
                "t · translations · T · themes · D · downloads · , · settings".to_string(),
                false,
                false,
            ),
            (
                "drag to select text · Ctrl+C copies · Ctrl+Shift+D moves the shortcut dock"
                    .to_string(),
                false,
                false,
            ),
            (
                "Space · action wheel when selected in settings · click a note to read it"
                    .to_string(),
                false,
                false,
            ),
            (
                "? · this help · q · quit and remember the reading position".to_string(),
                false,
                false,
            ),
        ],
        Screen::Reader => vec![],
    };

    v_flex().size_full().items_center().child(
        v_flex()
            .w_full()
            .max_w(px(860.))
            .flex_1()
            .min_h_0()
            .px(px(32.))
            .pt(px(30.))
            .pb(px(12.))
            .gap(px(18.))
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .gap(px(12.))
                    .child(
                        div()
                            .id("screen-back")
                            .size(px(34.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(9.))
                            .cursor_pointer()
                            .text_color(cx.theme().muted_foreground)
                            .hover(|style| {
                                style
                                    .bg(cx.theme().selection)
                                    .text_color(cx.theme().foreground)
                            })
                            .on_click(cx.listener(|this, _, _, cx| this.close_screen(cx)))
                            .child(Icon::new(IconName::ArrowLeft).size(px(18.))),
                    )
                    .child(
                        v_flex()
                            .gap(px(3.))
                            .child(div().text_size(px(22.)).child(title))
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(cx.theme().muted_foreground)
                                    .child(subtitle),
                            ),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(cx.theme().muted_foreground)
                            .child("Esc"),
                    ),
            )
            .child(
                v_flex()
                    .id("screen-list")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&app.screen_scroll)
                    .gap(px(2.))
                    .children(items.into_iter().enumerate().map(
                        |(i, (label, current, selected))| {
                            let screen = app.screen;
                            let mut row = div()
                                .id(SharedString::from(format!(
                                    "screen-row-{}-{i}",
                                    screen as usize
                                )))
                                .w_full()
                                .px(px(14.))
                                .py(px(11.))
                                .rounded(px(9.))
                                .text_size(px(14.))
                                .cursor_pointer()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if this.screen == Screen::Help {
                                        return;
                                    }
                                    match this.screen {
                                        Screen::Themes => this.theme_selected = i,
                                        Screen::Translations => this.translation_selected = i,
                                        Screen::Downloads => this.download_selected = i,
                                        Screen::Languages => this.language_selected = i,
                                        Screen::Study => this.study_selected = i,
                                        _ => {}
                                    }
                                    this.activate_screen_item(cx);
                                }))
                                .child(label);
                            if selected {
                                row = row
                                    .bg(cx.theme().selection)
                                    .text_color(cx.theme().foreground);
                            } else if current {
                                row = row.text_color(cx.theme().accent);
                            } else {
                                row = row
                                    .text_color(cx.theme().foreground)
                                    .hover(|style| style.bg(cx.theme().muted));
                            }
                            row
                        },
                    )),
            ),
    )
}
