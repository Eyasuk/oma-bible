use std::{fs, io, path::Path};

use serde::{Deserialize, Serialize};

use super::{BibleBook, LoadError};

/// A complete Bible translation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Translation {
    pub name: String,
    pub language: String,
    pub abbreviation: String,
    pub books: Vec<BibleBook>,
    /// Some printed Bibles join verses ("2-3"). Such sources leave the first
    /// verse empty and keep the text on the last one; the reader then labels
    /// the verse as a range.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub joined_verses: bool,
}

impl Translation {
    pub fn new(
        name: impl Into<String>,
        language: impl Into<String>,
        abbreviation: impl Into<String>,
        books: Vec<BibleBook>,
    ) -> Self {
        Self {
            name: name.into(),
            language: language.into(),
            abbreviation: abbreviation.into(),
            books,
            joined_verses: false,
        }
    }

    /// The USFM code of a book: its stored code, or one inferred from a
    /// standard 66, 39 (Old Testament) or 27 (New Testament) book layout.
    pub fn book_id(&self, index: usize) -> Option<String> {
        let book = self.books.get(index)?;
        if !book.id.is_empty() {
            return Some(book.id.clone());
        }
        let offset = match self.books.len() {
            66 | 39 => 0,
            27 => 39,
            _ => return None,
        };
        super::canon::BOOKS
            .get(offset + index)
            .map(|(id, _)| (*id).to_string())
    }

    /// Finds a book by its USFM code.
    pub fn book_index_by_id(&self, id: &str) -> Option<usize> {
        (0..self.books.len()).find(|index| {
            self.book_id(*index)
                .is_some_and(|book_id| book_id.eq_ignore_ascii_case(id))
        })
    }

    /// Reads a translation from a JSON file.
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, LoadError> {
        let path = path.as_ref();
        let contents = fs::read_to_string(path).map_err(|error| LoadError::read(path, error))?;

        serde_json::from_str(&contents).map_err(|error| LoadError::parse(path, error))
    }

    /// Writes the translation to a JSON file, creating parent directories.
    ///
    /// The file is written through a temporary file so a cancelled download
    /// never leaves a half-written translation behind.
    pub fn save(&self, path: impl AsRef<Path>) -> io::Result<()> {
        let path = path.as_ref();

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let contents = serde_json::to_vec_pretty(self).map_err(io::Error::other)?;
        let temporary = path.with_extension("json.tmp");
        fs::write(&temporary, contents)?;
        fs::rename(temporary, path)
    }

    /// Finds a book by name, ignoring ASCII letter case.
    pub fn book(&self, name: &str) -> Option<&BibleBook> {
        self.books
            .iter()
            .find(|book| book.name.eq_ignore_ascii_case(name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bible::Chapter;

    fn sample_translation() -> Translation {
        Translation::new(
            "King James Version",
            "English",
            "KJV",
            vec![BibleBook::new("Genesis", vec![Chapter::new(1, Vec::new())])],
        )
    }

    #[test]
    fn book_lookup_ignores_case() {
        let translation = sample_translation();

        assert_eq!(
            translation.book("genesis").map(|book| book.name.as_str()),
            Some("Genesis")
        );
        assert!(translation.book("Exodus").is_none());
    }

    #[test]
    fn loads_a_translation_from_json() {
        let path = crate::fixtures_dir().join("kjv.json");
        let translation = Translation::from_file(path).unwrap();

        assert_eq!(translation.abbreviation, "KJV");
        assert_eq!(
            translation
                .book("Genesis")
                .and_then(|book| book.chapter(1))
                .and_then(|chapter| chapter.verse(1)),
            Some("In the beginning God created the heaven and the earth.")
        );
    }
}
