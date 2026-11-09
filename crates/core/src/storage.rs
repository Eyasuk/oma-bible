//! Small JSON files for settings, bookmarks, and highlights.
//!
//! All files are optional: a missing or damaged file falls back to defaults so
//! a broken state file can never stop the reader from starting.

use std::{
    fs, io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

/// User preferences and the last reading position.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Theme slug selected when the app is not running under Omarchy.
    pub theme: Option<String>,
    /// Abbreviation of the last translation.
    pub translation: Option<String>,
    /// Name of the last book.
    pub book: Option<String>,
    /// Number of the last chapter.
    pub chapter: Option<u32>,
    /// 1-based number of the last selected verse.
    pub verse: Option<u32>,
    pub show_verse_numbers: bool,
    /// Reading text size used by the desktop front end.
    pub font_size: u32,
    /// Last language filter used in the download browser.
    pub download_language: Option<String>,
    /// Which quick-action control the reader shows: an edge dock or a wheel.
    pub controls: ControlsStyle,
    /// Edge of the window the shortcut dock is attached to.
    pub dock: DockPosition,
}

/// How the reader exposes bookmark, note, highlight, and settings actions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ControlsStyle {
    #[default]
    Dock,
    Wheel,
}

impl ControlsStyle {
    pub fn label(self) -> &'static str {
        match self {
            Self::Dock => "Dock",
            Self::Wheel => "Wheel",
        }
    }

    /// Next style in the settings toggle.
    pub fn next(self) -> Self {
        match self {
            Self::Dock => Self::Wheel,
            Self::Wheel => Self::Dock,
        }
    }
}

/// Where the desktop shortcut dock sits. The reader still owns the keyboard
/// shortcuts; this only positions the clickable strip.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DockPosition {
    Top,
    Left,
    Right,
    #[default]
    Bottom,
}

impl DockPosition {
    pub const ALL: [Self; 4] = [Self::Bottom, Self::Left, Self::Top, Self::Right];

    pub fn label(self) -> &'static str {
        match self {
            Self::Top => "Top",
            Self::Left => "Left",
            Self::Right => "Right",
            Self::Bottom => "Bottom",
        }
    }

    /// Next edge in the rotation used by the dock button.
    pub fn next(self) -> Self {
        match self {
            Self::Bottom => Self::Left,
            Self::Left => Self::Top,
            Self::Top => Self::Right,
            Self::Right => Self::Bottom,
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: None,
            translation: None,
            book: None,
            chapter: None,
            verse: None,
            show_verse_numbers: true,
            font_size: 17,
            download_language: None,
            controls: ControlsStyle::default(),
            dock: DockPosition::default(),
        }
    }
}

impl Settings {
    pub fn load() -> Self {
        Self::load_from(settings_path()).unwrap_or_default()
    }

    pub fn load_from(path: impl AsRef<Path>) -> Option<Self> {
        let contents = fs::read_to_string(path).ok()?;
        serde_json::from_str(&contents).ok()
    }

    pub fn save(&self) -> io::Result<()> {
        self.save_to(settings_path())
    }

    pub fn save_to(&self, path: impl AsRef<Path>) -> io::Result<()> {
        write_json(path.as_ref(), self)
    }
}

/// A verse the reader wants to come back to, optionally with a note.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bookmark {
    pub translation: String,
    pub book: String,
    pub chapter: u32,
    pub verse: u32,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub created: u64,
}

impl Bookmark {
    pub fn new(
        translation: impl Into<String>,
        book: impl Into<String>,
        chapter: u32,
        verse: u32,
    ) -> Self {
        Self {
            translation: translation.into(),
            book: book.into(),
            chapter,
            verse,
            note: String::new(),
            created: now_seconds(),
        }
    }

    /// `Genesis 1:1`, without the translation name.
    pub fn reference(&self) -> String {
        format!("{} {}:{}", self.book, self.chapter, self.verse)
    }
}

/// All bookmarks, in the order they were added.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Bookmarks {
    items: Vec<Bookmark>,
}

impl Bookmarks {
    pub fn load() -> Self {
        Self::load_from(bookmarks_path()).unwrap_or_default()
    }

    pub fn load_from(path: impl AsRef<Path>) -> Option<Self> {
        let contents = fs::read_to_string(path).ok()?;
        serde_json::from_str(&contents).ok()
    }

    pub fn save(&self) -> io::Result<()> {
        self.save_to(bookmarks_path())
    }

    pub fn save_to(&self, path: impl AsRef<Path>) -> io::Result<()> {
        write_json(path.as_ref(), self)
    }

    pub fn items(&self) -> &[Bookmark] {
        &self.items
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn find(
        &self,
        translation: &str,
        book: &str,
        chapter: u32,
        verse: u32,
    ) -> Option<&Bookmark> {
        self.items.iter().find(|bookmark| {
            bookmark.translation.eq_ignore_ascii_case(translation)
                && bookmark.book.eq_ignore_ascii_case(book)
                && bookmark.chapter == chapter
                && bookmark.verse == verse
        })
    }

    /// Adds the bookmark, or removes it when it already exists.
    ///
    /// Returns `true` when the verse is now bookmarked.
    pub fn toggle(&mut self, translation: &str, book: &str, chapter: u32, verse: u32) -> bool {
        if self.remove(translation, book, chapter, verse) {
            return false;
        }

        self.items
            .push(Bookmark::new(translation, book, chapter, verse));
        true
    }

    /// Creates the bookmark when needed and replaces its note.
    pub fn set_note(
        &mut self,
        translation: &str,
        book: &str,
        chapter: u32,
        verse: u32,
        note: &str,
    ) {
        match self.items.iter_mut().find(|bookmark| {
            bookmark.translation.eq_ignore_ascii_case(translation)
                && bookmark.book.eq_ignore_ascii_case(book)
                && bookmark.chapter == chapter
                && bookmark.verse == verse
        }) {
            Some(bookmark) => bookmark.note = note.trim().to_string(),
            None => {
                let mut bookmark = Bookmark::new(translation, book, chapter, verse);
                bookmark.note = note.trim().to_string();
                self.items.push(bookmark);
            }
        }
    }

    pub fn remove(&mut self, translation: &str, book: &str, chapter: u32, verse: u32) -> bool {
        let before = self.items.len();
        self.items.retain(|bookmark| {
            !(bookmark.translation.eq_ignore_ascii_case(translation)
                && bookmark.book.eq_ignore_ascii_case(book)
                && bookmark.chapter == chapter
                && bookmark.verse == verse)
        });
        before != self.items.len()
    }

    pub fn remove_index(&mut self, index: usize) -> Option<Bookmark> {
        (index < self.items.len()).then(|| self.items.remove(index))
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }
}

/// The small set of colours a verse can be highlighted with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HighlightColor {
    Yellow,
    Green,
    Blue,
    Pink,
}

impl HighlightColor {
    pub const ALL: [Self; 4] = [Self::Yellow, Self::Green, Self::Blue, Self::Pink];

    pub fn label(self) -> &'static str {
        match self {
            Self::Yellow => "Yellow",
            Self::Green => "Green",
            Self::Blue => "Blue",
            Self::Pink => "Pink",
        }
    }
}

/// A colour applied to a verse, independent of bookmarks and notes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Highlight {
    pub translation: String,
    pub book: String,
    pub chapter: u32,
    pub verse: u32,
    pub color: HighlightColor,
    #[serde(default)]
    pub created: u64,
}

impl Highlight {
    pub fn new(
        translation: impl Into<String>,
        book: impl Into<String>,
        chapter: u32,
        verse: u32,
        color: HighlightColor,
    ) -> Self {
        Self {
            translation: translation.into(),
            book: book.into(),
            chapter,
            verse,
            color,
            created: now_seconds(),
        }
    }

    /// `Genesis 1:1`, without the translation name.
    pub fn reference(&self) -> String {
        format!("{} {}:{}", self.book, self.chapter, self.verse)
    }
}

/// All verse highlights, in the order they were first added.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Highlights {
    items: Vec<Highlight>,
}

impl Highlights {
    pub fn load() -> Self {
        Self::load_from(highlights_path()).unwrap_or_default()
    }

    pub fn load_from(path: impl AsRef<Path>) -> Option<Self> {
        let contents = fs::read_to_string(path).ok()?;
        serde_json::from_str(&contents).ok()
    }

    pub fn save(&self) -> io::Result<()> {
        self.save_to(highlights_path())
    }

    pub fn save_to(&self, path: impl AsRef<Path>) -> io::Result<()> {
        write_json(path.as_ref(), self)
    }

    pub fn items(&self) -> &[Highlight] {
        &self.items
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn find(
        &self,
        translation: &str,
        book: &str,
        chapter: u32,
        verse: u32,
    ) -> Option<&Highlight> {
        self.items.iter().find(|highlight| {
            highlight.translation.eq_ignore_ascii_case(translation)
                && highlight.book.eq_ignore_ascii_case(book)
                && highlight.chapter == chapter
                && highlight.verse == verse
        })
    }

    /// Applies a colour, replacing the existing one when the verse is already
    /// highlighted.
    pub fn set(
        &mut self,
        translation: &str,
        book: &str,
        chapter: u32,
        verse: u32,
        color: HighlightColor,
    ) {
        match self.items.iter_mut().find(|highlight| {
            highlight.translation.eq_ignore_ascii_case(translation)
                && highlight.book.eq_ignore_ascii_case(book)
                && highlight.chapter == chapter
                && highlight.verse == verse
        }) {
            Some(highlight) => highlight.color = color,
            None => self
                .items
                .push(Highlight::new(translation, book, chapter, verse, color)),
        }
    }

    pub fn remove(&mut self, translation: &str, book: &str, chapter: u32, verse: u32) -> bool {
        let before = self.items.len();
        self.items.retain(|highlight| {
            !(highlight.translation.eq_ignore_ascii_case(translation)
                && highlight.book.eq_ignore_ascii_case(book)
                && highlight.chapter == chapter
                && highlight.verse == verse)
        });
        before != self.items.len()
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }
}

pub fn settings_path() -> PathBuf {
    crate::config_dir().join("settings.json")
}

pub fn bookmarks_path() -> PathBuf {
    crate::state_dir().join("bookmarks.json")
}

pub fn highlights_path() -> PathBuf {
    crate::state_dir().join("highlights.json")
}

fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let contents = serde_json::to_vec_pretty(value).map_err(io::Error::other)?;
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, contents)?;
    fs::rename(temporary, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_FILE: AtomicU64 = AtomicU64::new(0);

    fn temporary_file(name: &str) -> PathBuf {
        let id = NEXT_FILE.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "oma-bible-{}-{}-{name}.json",
            std::process::id(),
            id
        ))
    }

    #[test]
    fn missing_settings_fields_use_defaults() {
        let settings: Settings = serde_json::from_str(r#"{"show_verse_numbers": false}"#).unwrap();

        assert!(!settings.show_verse_numbers);
        assert_eq!(settings.font_size, 17);
        assert!(settings.translation.is_none());
    }

    #[test]
    fn settings_round_trip_through_a_file() {
        let path = temporary_file("settings");
        let settings = Settings {
            translation: Some(String::from("KJV")),
            book: Some(String::from("Genesis")),
            chapter: Some(3),
            ..Settings::default()
        };

        settings.save_to(&path).unwrap();
        let loaded = Settings::load_from(&path).unwrap();
        fs::remove_file(path).unwrap();

        assert_eq!(loaded, settings);
    }

    #[test]
    fn toggling_a_bookmark_adds_then_removes_it() {
        let mut bookmarks = Bookmarks::default();

        assert!(bookmarks.toggle("KJV", "Genesis", 1, 1));
        assert!(bookmarks.find("kjv", "genesis", 1, 1).is_some());
        assert!(!bookmarks.toggle("KJV", "Genesis", 1, 1));
        assert!(bookmarks.is_empty());
    }

    #[test]
    fn setting_a_note_creates_the_bookmark() {
        let mut bookmarks = Bookmarks::default();

        bookmarks.set_note("KJV", "John", 3, 16, "  grace  ");

        let bookmark = bookmarks.find("KJV", "John", 3, 16).unwrap();
        assert_eq!(bookmark.note, "grace");
        assert_eq!(bookmark.reference(), "John 3:16");
    }

    #[test]
    fn setting_a_highlight_replaces_the_colour() {
        let mut highlights = Highlights::default();

        highlights.set("KJV", "John", 3, 16, HighlightColor::Yellow);
        highlights.set("kjv", "john", 3, 16, HighlightColor::Blue);

        assert_eq!(highlights.len(), 1);
        assert_eq!(
            highlights.find("KJV", "John", 3, 16).unwrap().color,
            HighlightColor::Blue
        );
        assert!(highlights.remove("KJV", "John", 3, 16));
        assert!(highlights.is_empty());
    }

    #[test]
    fn highlights_round_trip_through_a_file() {
        let path = temporary_file("highlights");
        let mut highlights = Highlights::default();
        highlights.set("KJV", "Psalm", 23, 1, HighlightColor::Green);

        highlights.save_to(&path).unwrap();
        let loaded = Highlights::load_from(&path).unwrap();
        fs::remove_file(path).unwrap();

        assert_eq!(loaded, highlights);
        assert_eq!(loaded.items()[0].reference(), "Psalm 23:1");
    }

    #[test]
    fn bookmarks_round_trip_through_a_file() {
        let path = temporary_file("bookmarks");
        let mut bookmarks = Bookmarks::default();
        bookmarks.toggle("KJV", "Psalm", 23, 1);
        bookmarks.set_note("KJV", "Psalm", 23, 1, "comfort");

        bookmarks.save_to(&path).unwrap();
        let loaded = Bookmarks::load_from(&path).unwrap();
        fs::remove_file(path).unwrap();

        assert_eq!(loaded, bookmarks);
        assert_eq!(loaded.items()[0].note, "comfort");
    }
}
