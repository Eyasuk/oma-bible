use serde::{Deserialize, Serialize};

use super::Chapter;

/// A book of the Bible, such as Genesis or Exodus.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BibleBook {
    pub name: String,
    pub chapters: Vec<Chapter>,
}

impl BibleBook {
    pub fn new(name: impl Into<String>, chapters: Vec<Chapter>) -> Self {
        Self {
            name: name.into(),
            chapters,
        }
    }

    /// Returns the chapter with the given number.
    pub fn chapter(&self, number: u32) -> Option<&Chapter> {
        self.chapters
            .iter()
            .find(|chapter| chapter.number == number)
    }

    pub fn chapter_count(&self) -> usize {
        self.chapters.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_book() -> BibleBook {
        BibleBook::new(
            "Genesis",
            vec![Chapter::new(1, Vec::new()), Chapter::new(2, Vec::new())],
        )
    }

    #[test]
    fn chapter_returns_the_matching_chapter() {
        let book = sample_book();

        assert_eq!(book.chapter(2).map(|chapter| chapter.number), Some(2));
        assert!(book.chapter(3).is_none());
    }

    #[test]
    fn chapter_count_matches_the_chapter_list() {
        assert_eq!(sample_book().chapter_count(), 2);
    }
}
