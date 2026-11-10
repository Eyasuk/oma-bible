//! The original Hebrew and Greek words behind each verse.
//!
//! Word-by-word data comes from the Berean Standard Bible interlinear
//! (public domain): each word's Hebrew or Greek text, transliteration,
//! parsing, Strong's number and English gloss. Definitions come from the
//! STEPBible brief lexicons (TBESH/TBESG, CC BY 4.0, STEPBible.org / Tyndale
//! House). Both are served by the Free Use Bible API and cached on disk like
//! the other study data.

use std::collections::HashMap;

use serde::Deserialize;

use crate::remote::{API_BASE, RemoteError};
use crate::study::{cached, safe};

/// One word of the original text.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct OriginalWord {
    /// Position in the Hebrew or Greek text.
    #[serde(default)]
    pub pos: u32,
    /// Position of the gloss in the BSB English sentence.
    #[serde(default, rename = "bsbPos")]
    pub bsb_pos: Option<u32>,
    /// `hebrew`, `aramaic` or `greek`.
    #[serde(default)]
    pub language: String,
    pub text: String,
    #[serde(default)]
    pub transliteration: Option<String>,
    #[serde(default)]
    pub parsing: Option<String>,
    #[serde(default, rename = "parsingFull")]
    pub parsing_full: Option<String>,
    /// Strong's number such as `H7225` or `G3056`.
    #[serde(default)]
    pub strongs: Option<String>,
    /// English for this word in the BSB; empty for untranslated words such
    /// as the Hebrew object marker.
    #[serde(default)]
    pub gloss: Option<String>,
}

impl OriginalWord {
    /// Hebrew and Aramaic read right to left.
    pub fn is_rtl(&self) -> bool {
        crate::direction::direction_of(&self.text).is_rtl()
    }
}

/// The original words of one chapter, keyed by verse number.
pub type InterlinearChapter = HashMap<u32, Vec<OriginalWord>>;

#[derive(Deserialize)]
struct InterlinearFile {
    chapter: InterlinearContent,
}

#[derive(Deserialize)]
struct InterlinearContent {
    #[serde(default)]
    content: Vec<InterlinearVerse>,
}

#[derive(Deserialize)]
struct InterlinearVerse {
    verse: u32,
    #[serde(default)]
    words: Vec<OriginalWord>,
}

/// The original words of a chapter (`book` is a USFM code).
pub fn interlinear(book: &str, chapter: u32) -> Result<InterlinearChapter, RemoteError> {
    let book = safe(book);
    let Some(body) = cached(
        &format!("{API_BASE}/api/d/bsb-interlinear/{book}/{chapter}.json"),
        &format!("bsb-interlinear/{book}/{chapter}.json"),
    )?
    else {
        return Ok(InterlinearChapter::new());
    };
    let file: InterlinearFile = serde_json::from_slice(&body)?;
    Ok(file
        .chapter
        .content
        .into_iter()
        .map(|verse| (verse.verse, verse.words))
        .collect())
}

/// A lexicon entry: one Strong's number can have several senses.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LexiconEntry {
    pub id: String,
    #[serde(default)]
    pub senses: Vec<Sense>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Sense {
    #[serde(default)]
    pub lemma: String,
    #[serde(default)]
    pub transliteration: String,
    #[serde(default, rename = "partOfSpeech")]
    pub part_of_speech: String,
    #[serde(default)]
    pub gloss: String,
    #[serde(default)]
    pub definition: String,
}

impl Sense {
    /// The definition as paragraphs, without the source's `__` outline
    /// markers.
    pub fn paragraphs(&self) -> Vec<String> {
        self.definition
            .split('\n')
            .map(|line| line.trim().trim_start_matches('_').trim())
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect()
    }
}

#[derive(Deserialize)]
struct LexiconFile {
    entry: LexiconEntry,
}

/// The lexicon entry for a Strong's number, or `None` if there is none.
pub fn lexicon(strongs: &str) -> Result<Option<LexiconEntry>, RemoteError> {
    let id = safe(strongs);
    // The built-in lexicon keeps a hundred entries per block.
    if let Some(block) =
        crate::bundle::lexicon_block_key(&id).and_then(|key| crate::bundle::get(&key))
    {
        let mut entries: HashMap<String, LexiconEntry> = serde_json::from_slice(&block)?;
        return Ok(entries.remove(&id));
    }
    let Some(body) = cached(
        &format!("{API_BASE}/api/d/stepbible-lexicon/lexicon/{id}.json"),
        &format!("stepbible-lexicon/{id}.json"),
    )?
    else {
        return Ok(None);
    };
    let file: LexiconFile = serde_json::from_slice(&body)?;
    Ok(Some(file.entry))
}

/// A word clicked in a verse of the open translation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClickedWord {
    pub text: String,
    /// Which word of the verse it is, from 0.
    pub index: usize,
    /// How many words the verse has.
    pub count: usize,
}

/// Picks the original word behind a word clicked in a translation.
///
/// A Hebrew or Greek translation is matched on the original text itself.
/// Other translations are matched on the English glosses, so this works
/// for English translations; when several glosses fit, the one at the same
/// place in the sentence wins. Words in other languages cannot be linked
/// to the original and give `None`.
pub fn match_word(clicked: &ClickedWord, words: &[OriginalWord]) -> Option<usize> {
    let token = fold(&clicked.text);
    if token.is_empty() || words.is_empty() {
        return None;
    }

    if let Some(index) = words.iter().position(|word| fold(&word.text) == token) {
        return Some(index);
    }
    if !token.chars().any(|c| c.is_ascii_alphabetic()) {
        return None;
    }

    // Where the clicked word sits in its verse, and where each gloss sits
    // in the BSB sentence, both from 0 to 1.
    let place = |index: usize, count: usize| index as f32 / count.max(2).saturating_sub(1) as f32;
    let clicked_place = place(clicked.index, clicked.count);
    let mut english_order: Vec<usize> = (0..words.len()).collect();
    english_order.sort_by_key(|&index| words[index].bsb_pos.unwrap_or(words[index].pos));
    let mut rank = vec![0; words.len()];
    for (order, &index) in english_order.iter().enumerate() {
        rank[index] = order;
    }
    let distance = |index: usize| (place(rank[index], words.len()) - clicked_place).abs();

    let stem = stem(&token);
    let score = |word: &OriginalWord| -> u8 {
        let gloss = word.gloss.as_deref().unwrap_or_default();
        let mut best = 0;
        for part in gloss.split(|c: char| !c.is_alphanumeric() && c != '\'') {
            let part = fold(part);
            if part.is_empty() {
                continue;
            }
            if part == token {
                return 2;
            }
            if self::stem(&part) == stem {
                best = 1;
            }
        }
        best
    };

    let best = (0..words.len())
        .map(|index| (index, score(&words[index])))
        .filter(|(_, score)| *score > 0)
        .min_by(|(a, score_a), (b, score_b)| {
            score_b
                .cmp(score_a)
                .then(distance(*a).total_cmp(&distance(*b)))
        });
    match best {
        Some((index, _)) => Some(index),
        // No gloss fits (an older English word, say): take the word at the
        // same place in the sentence.
        None => (0..words.len()).min_by(|a, b| distance(*a).total_cmp(&distance(*b))),
    }
}

/// Lowercase, without punctuation or combining marks (Hebrew points and
/// cantillation, decomposed Greek accents).
fn fold(text: &str) -> String {
    // Hebrew points count as alphabetic in Unicode, so drop marks by range.
    let is_mark =
        |c: char| matches!(c as u32, 0x0300..=0x036F | 0x0591..=0x05C7 | 0x064B..=0x065F | 0x0670);
    text.chars()
        .filter(|&c| c.is_alphanumeric() && !is_mark(c))
        .flat_map(char::to_lowercase)
        .collect()
}

/// A rough English stem so `heaven` meets `heavens` and `created` meets
/// `create`, and older forms like `saith` meet `says`.
fn stem(word: &str) -> &str {
    for suffix in ["eth", "est", "ing", "ed", "es", "s", "th", "e"] {
        if let Some(stem) = word.strip_suffix(suffix)
            && stem.chars().count() >= 3
        {
            return stem;
        }
    }
    word
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(pos: u32, bsb: u32, text: &str, gloss: Option<&str>) -> OriginalWord {
        OriginalWord {
            pos,
            bsb_pos: Some(bsb),
            language: "hebrew".into(),
            text: text.into(),
            transliteration: None,
            parsing: None,
            parsing_full: None,
            strongs: None,
            gloss: gloss.map(str::to_string),
        }
    }

    /// Genesis 1:1 in Hebrew order, with the BSB English order.
    fn genesis() -> Vec<OriginalWord> {
        vec![
            word(1, 1, "בְּרֵאשִׁ֖ית", Some("In the beginning")),
            word(2, 4, "בָּרָ֣א", Some("created")),
            word(3, 2, "אֱלֹהִ֑ים", Some("God")),
            word(4, 3, "אֵ֥ת", None),
            word(5, 5, "הַשָּׁמַ֖יִם", Some("the heavens")),
            word(6, 6, "וְאֵ֥ת", Some("and")),
            word(7, 7, "הָאָֽרֶץ׃", Some("the earth")),
        ]
    }

    fn clicked(verse: &str, index: usize) -> ClickedWord {
        let words: Vec<&str> = verse.split_whitespace().collect();
        ClickedWord {
            text: words[index].to_string(),
            index,
            count: words.len(),
        }
    }

    const KJV: &str = "In the beginning God created the heaven and the earth.";

    #[test]
    fn matches_english_glosses() {
        let words = genesis();
        assert_eq!(match_word(&clicked(KJV, 3), &words), Some(2)); // God
        assert_eq!(match_word(&clicked(KJV, 4), &words), Some(1)); // created
        assert_eq!(match_word(&clicked(KJV, 6), &words), Some(4)); // heaven
        assert_eq!(match_word(&clicked(KJV, 9), &words), Some(6)); // earth.
    }

    #[test]
    fn a_repeated_word_takes_the_one_at_the_same_place() {
        let words = genesis();
        assert_eq!(match_word(&clicked(KJV, 1), &words), Some(0)); // first "the"
        assert_eq!(match_word(&clicked(KJV, 8), &words), Some(6)); // last "the"
    }

    #[test]
    fn matches_the_original_text_without_points() {
        let words = genesis();
        let hebrew = ClickedWord {
            text: "אלהים".into(),
            index: 2,
            count: 7,
        };
        assert_eq!(match_word(&hebrew, &words), Some(2));
    }

    #[test]
    fn other_languages_are_not_guessed() {
        let words = genesis();
        let amharic = ClickedWord {
            text: "እግዚአብሔር".into(),
            index: 2,
            count: 6,
        };
        assert_eq!(match_word(&amharic, &words), None);
    }

    #[test]
    fn definitions_lose_outline_markers() {
        let sense = Sense {
            lemma: String::new(),
            transliteration: String::new(),
            part_of_speech: String::new(),
            gloss: String::new(),
            definition: "λόγος\n\n__I. Of that\n\n__1. a word".into(),
        };
        assert_eq!(sense.paragraphs(), ["λόγος", "I. Of that", "1. a word"]);
    }
}
