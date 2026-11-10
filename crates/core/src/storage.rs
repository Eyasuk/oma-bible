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
    /// Edge of the window the shortcut dock is attached to.
    pub dock: DockPosition,
    /// Commentary shown in the study sheet, by API id.
    pub commentary: Option<String>,
}

/// Where the desktop shortcut dock floats. The reader still owns the keyboard
/// shortcuts and the right-click wheel; this only positions (or hides) the
/// clickable pill.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DockPosition {
    Top,
    Left,
    Right,
    #[default]
    Bottom,
    Hidden,
}

impl DockPosition {
    pub const ALL: [Self; 5] = [
        Self::Bottom,
        Self::Left,
        Self::Top,
        Self::Right,
        Self::Hidden,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Top => "Top",
            Self::Left => "Left",
            Self::Right => "Right",
            Self::Bottom => "Bottom",
            Self::Hidden => "Hidden",
        }
    }

    /// Next edge in the rotation used by the dock button.
    pub fn next(self) -> Self {
        match self {
            Self::Bottom => Self::Left,
            Self::Left => Self::Top,
            Self::Top => Self::Right,
            Self::Right => Self::Hidden,
            Self::Hidden => Self::Bottom,
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
            dock: DockPosition::default(),
            commentary: None,
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

/// One study note on a verse. A verse can carry any number of notes; each
/// may quote other verses inline as plain text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StudyNote {
    pub id: u64,
    pub translation: String,
    pub book: String,
    pub chapter: u32,
    pub verse: u32,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub created: u64,
    #[serde(default)]
    pub updated: u64,
}

impl StudyNote {
    /// `John 3:16`, without the translation name.
    pub fn reference(&self) -> String {
        format!("{} {}:{}", self.book, self.chapter, self.verse)
    }

    fn is_on(&self, translation: &str, book: &str, chapter: u32) -> bool {
        self.translation.eq_ignore_ascii_case(translation)
            && self.book.eq_ignore_ascii_case(book)
            && self.chapter == chapter
    }
}

/// Every study note, in the order they were written.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Notes {
    items: Vec<StudyNote>,
}

impl Notes {
    /// Loads `notes.json`. The first time it is missing, the single notes
    /// kept on bookmarks are copied in so nothing written earlier is lost.
    pub fn load_or_import(bookmarks: &Bookmarks) -> Self {
        let path = notes_path();
        if let Some(notes) = Self::load_from(&path) {
            return notes;
        }
        let mut notes = Self::default();
        if path.exists() {
            // A damaged file: start empty rather than overwrite it.
            return notes;
        }
        notes.import_bookmark_notes(bookmarks);
        if !notes.is_empty() {
            let _ = notes.save_to(&path);
        }
        notes
    }

    pub fn load_from(path: impl AsRef<Path>) -> Option<Self> {
        let contents = fs::read_to_string(path).ok()?;
        serde_json::from_str(&contents).ok()
    }

    pub fn save(&self) -> io::Result<()> {
        self.save_to(notes_path())
    }

    pub fn save_to(&self, path: impl AsRef<Path>) -> io::Result<()> {
        write_json(path.as_ref(), self)
    }

    pub fn items(&self) -> &[StudyNote] {
        &self.items
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn get(&self, id: u64) -> Option<&StudyNote> {
        self.items.iter().find(|note| note.id == id)
    }

    /// Notes on one verse, oldest first.
    pub fn for_verse(
        &self,
        translation: &str,
        book: &str,
        chapter: u32,
        verse: u32,
    ) -> Vec<&StudyNote> {
        self.items
            .iter()
            .filter(|note| note.is_on(translation, book, chapter) && note.verse == verse)
            .collect()
    }

    /// Notes in one chapter, in verse order and then oldest first.
    pub fn for_chapter(&self, translation: &str, book: &str, chapter: u32) -> Vec<&StudyNote> {
        let mut notes: Vec<&StudyNote> = self
            .items
            .iter()
            .filter(|note| note.is_on(translation, book, chapter))
            .collect();
        notes.sort_by_key(|note| (note.verse, note.created, note.id));
        notes
    }

    /// Adds a note and returns its id. Blank notes are not stored.
    pub fn add(
        &mut self,
        translation: &str,
        book: &str,
        chapter: u32,
        verse: u32,
        body: &str,
    ) -> Option<u64> {
        let body = body.trim();
        if body.is_empty() {
            return None;
        }
        let id = self.next_id();
        let now = now_seconds();
        self.items.push(StudyNote {
            id,
            translation: translation.to_string(),
            book: book.to_string(),
            chapter,
            verse,
            body: body.to_string(),
            created: now,
            updated: now,
        });
        Some(id)
    }

    /// Replaces a note's text; an emptied note is removed.
    pub fn update(&mut self, id: u64, body: &str) -> bool {
        let body = body.trim();
        if body.is_empty() {
            return self.remove(id);
        }
        match self.items.iter_mut().find(|note| note.id == id) {
            Some(note) => {
                note.body = body.to_string();
                note.updated = now_seconds();
                true
            }
            None => false,
        }
    }

    pub fn remove(&mut self, id: u64) -> bool {
        let before = self.items.len();
        self.items.retain(|note| note.id != id);
        before != self.items.len()
    }

    fn next_id(&self) -> u64 {
        self.items.iter().map(|note| note.id).max().unwrap_or(0) + 1
    }

    fn import_bookmark_notes(&mut self, bookmarks: &Bookmarks) {
        for bookmark in bookmarks.items() {
            let body = bookmark.note.trim();
            if body.is_empty() {
                continue;
            }
            let id = self.next_id();
            self.items.push(StudyNote {
                id,
                translation: bookmark.translation.clone(),
                book: bookmark.book.clone(),
                chapter: bookmark.chapter,
                verse: bookmark.verse,
                body: body.to_string(),
                created: bookmark.created,
                updated: bookmark.created,
            });
        }
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

pub fn notes_path() -> PathBuf {
    crate::state_dir().join("notes.json")
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

    #[test]
    fn a_verse_holds_several_notes() {
        let mut notes = Notes::default();
        let first = notes.add("KJV", "John", 3, 16, "love").unwrap();
        let second = notes.add("kjv", "john", 3, 16, "gift").unwrap();
        notes.add("KJV", "John", 3, 1, "Nicodemus").unwrap();
        assert!(notes.add("KJV", "John", 3, 2, "   ").is_none());

        assert_ne!(first, second);
        assert_eq!(notes.for_verse("KJV", "John", 3, 16).len(), 2);
        let chapter: Vec<u32> = notes
            .for_chapter("KJV", "John", 3)
            .iter()
            .map(|note| note.verse)
            .collect();
        assert_eq!(chapter, vec![1, 16, 16]);

        assert!(notes.update(first, "God so loved"));
        assert_eq!(notes.get(first).unwrap().body, "God so loved");
        assert!(notes.update(second, ""));
        assert_eq!(notes.len(), 2);
    }

    #[test]
    fn bookmark_notes_are_imported_once() {
        let mut bookmarks = Bookmarks::default();
        bookmarks.set_note("KJV", "Psalm", 23, 1, "comfort");
        bookmarks.toggle("KJV", "Psalm", 23, 2);
        let mut notes = Notes::default();
        notes.import_bookmark_notes(&bookmarks);

        assert_eq!(notes.len(), 1);
        assert_eq!(notes.items()[0].body, "comfort");
        assert_eq!(notes.items()[0].reference(), "Psalm 23:1");
    }
}
