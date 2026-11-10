//! Writing direction for right-to-left scripts.
//!
//! Arabic, Hebrew, Persian, Urdu, Syriac and a few other scripts are read
//! right to left. The direction is worked out from the text itself (the first
//! letter with a strong direction, as in the Unicode bidi algorithm), so it
//! works for any translation without a language table.

use crate::bible::Translation;

/// The direction a piece of text is read in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Direction {
    #[default]
    Ltr,
    Rtl,
}

impl Direction {
    pub fn is_rtl(self) -> bool {
        self == Direction::Rtl
    }
}

/// Whether `c` belongs to a right-to-left script (Hebrew, Arabic, Syriac,
/// Thaana, N'Ko, Samaritan, Mandaic, their presentation forms, and the
/// historic RTL scripts in the supplementary planes).
pub fn is_rtl_char(c: char) -> bool {
    matches!(
        c as u32,
        0x0590..=0x08FF
            | 0xFB1D..=0xFDFF
            | 0xFE70..=0xFEFF
            | 0x10800..=0x10FFF
            | 0x1E800..=0x1EFFF
    )
}

/// Whether `c` has a strong left-to-right direction (a letter from any
/// other script). Digits, punctuation and combining marks are neutral.
fn is_ltr_char(c: char) -> bool {
    !is_rtl_char(c) && c.is_alphabetic()
}

/// The direction of `text`, from its first strongly directional letter.
/// Text with no letters at all is left to right.
pub fn direction_of(text: &str) -> Direction {
    for c in text.chars() {
        if is_rtl_char(c) {
            return Direction::Rtl;
        }
        if is_ltr_char(c) {
            return Direction::Ltr;
        }
    }
    Direction::Ltr
}

/// Whether `text` contains any right-to-left letter.
pub fn has_rtl(text: &str) -> bool {
    text.chars().any(is_rtl_char)
}

impl Translation {
    /// The reading direction of the translation, judged from the letters of
    /// its first few verses so a stray Latin word cannot flip it.
    pub fn direction(&self) -> Direction {
        let (mut rtl, mut ltr) = (0usize, 0usize);
        let verses = self
            .books
            .iter()
            .flat_map(|book| book.chapters.iter())
            .flat_map(|chapter| chapter.verses.iter())
            .filter(|verse| !verse.trim().is_empty())
            .take(8);
        for verse in verses {
            for c in verse.chars() {
                if is_rtl_char(c) {
                    rtl += 1;
                } else if is_ltr_char(c) {
                    ltr += 1;
                }
            }
        }
        if rtl == 0 && ltr == 0 {
            // No verses yet: fall back to the name.
            return direction_of(&format!("{} {}", self.name, self.language));
        }
        if rtl > ltr {
            Direction::Rtl
        } else {
            Direction::Ltr
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bible::{BibleBook, Chapter};

    #[test]
    fn detects_the_first_strong_letter() {
        assert_eq!(direction_of("فِي الْبَدْءِ خَلَقَ اللهُ"), Direction::Rtl);
        assert_eq!(direction_of("בְּרֵאשִׁית בָּרָא"), Direction::Rtl);
        assert_eq!(direction_of("In the beginning"), Direction::Ltr);
        assert_eq!(direction_of("በመጀመሪያ"), Direction::Ltr);
        assert_eq!(direction_of("12 (« اللهُ"), Direction::Rtl);
        assert_eq!(direction_of("12 — ."), Direction::Ltr);
    }

    #[test]
    fn translation_direction_follows_its_verses() {
        let arabic = Translation::new(
            "كتاب الحياة",
            "Arabic",
            "ARBNAV",
            vec![BibleBook::new(
                "التكوين",
                vec![Chapter::new(
                    1,
                    vec!["فِي الْبَدْءِ خَلَقَ اللهُ السَّمَاوَاتِ (God) وَالأَرْضَ".to_string()],
                )],
            )],
        );
        assert!(arabic.direction().is_rtl());

        let english = Translation::new(
            "King James Version",
            "English",
            "KJV",
            vec![BibleBook::new(
                "Genesis",
                vec![Chapter::new(1, vec!["In the beginning".to_string()])],
            )],
        );
        assert_eq!(english.direction(), Direction::Ltr);
    }
}
