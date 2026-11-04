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
        }
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
        let path = crate::data_dir().join("kjv.json");
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
