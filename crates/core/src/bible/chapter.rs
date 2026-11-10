use serde::{Deserialize, Serialize};

/// A single chapter of a Bible book.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Chapter {
    pub number: u32,
    /// Verses in reading order. The first item is verse 1.
    pub verses: Vec<String>,
    /// Section headings, each shown before the verse it names.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headings: Vec<Heading>,
    /// Translator footnotes, attached to verses.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub footnotes: Vec<Footnote>,
}

/// A section heading placed before `verse` (1-based).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Heading {
    pub verse: u32,
    pub text: String,
}

/// A footnote on `verse` (1-based).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Footnote {
    pub verse: u32,
    pub text: String,
}

impl Chapter {
    pub fn new(number: u32, verses: Vec<String>) -> Self {
        Self {
            number,
            verses,
            headings: Vec::new(),
            footnotes: Vec::new(),
        }
    }

    /// Headings shown before a verse (1-based).
    pub fn headings_before(&self, verse: u32) -> impl Iterator<Item = &str> {
        self.headings
            .iter()
            .filter(move |heading| heading.verse == verse)
            .map(|heading| heading.text.as_str())
    }

    /// Footnotes on a verse (1-based).
    pub fn footnotes_for(&self, verse: u32) -> impl Iterator<Item = &str> {
        self.footnotes
            .iter()
            .filter(move |note| note.verse == verse)
            .map(|note| note.text.as_str())
    }

    /// Returns the verse with the given 1-based number.
    pub fn verse(&self, number: u32) -> Option<&str> {
        let index = number.checked_sub(1)? as usize;
        self.verses.get(index).map(String::as_str)
    }

    pub fn verse_count(&self) -> usize {
        self.verses.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_chapter() -> Chapter {
        Chapter::new(1, vec![String::from("one"), String::from("two")])
    }

    #[test]
    fn verse_numbers_start_at_one() {
        let chapter = sample_chapter();

        assert_eq!(chapter.verse(1), Some("one"));
        assert_eq!(chapter.verse(2), Some("two"));
        assert_eq!(chapter.verse(0), None);
        assert_eq!(chapter.verse(3), None);
    }

    #[test]
    fn verse_count_matches_the_verse_list() {
        assert_eq!(sample_chapter().verse_count(), 2);
    }
}
