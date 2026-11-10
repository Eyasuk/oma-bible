//! Study data from the Free Use Bible API: cross-references from the
//! `open-cross-ref` dataset and verse commentaries.
//!
//! Each chapter is fetched once and kept in the cache directory, so it works
//! offline afterwards.

use std::{collections::HashMap, fs, path::PathBuf};

use serde::Deserialize;
use serde_json::Value;

use crate::remote::{API_BASE, RemoteError, http_get};

/// A verse (or verse range) another verse points to.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CrossReference {
    /// USFM book code, e.g. `ROM`.
    pub book: String,
    pub chapter: u32,
    pub verse: u32,
    #[serde(default, rename = "endVerse")]
    pub end_verse: Option<u32>,
    /// How many readers linked the two verses; higher is stronger.
    #[serde(default)]
    pub score: i64,
}

/// Cross-references of one chapter, keyed by verse number.
pub type ChapterReferences = HashMap<u32, Vec<CrossReference>>;

#[derive(Deserialize)]
struct ReferencesFile {
    chapter: ReferencesChapter,
}

#[derive(Deserialize)]
struct ReferencesChapter {
    #[serde(default)]
    content: Vec<VerseReferences>,
}

#[derive(Deserialize)]
struct VerseReferences {
    verse: u32,
    #[serde(default)]
    references: Vec<CrossReference>,
}

/// Cross-references for a chapter, strongest first for each verse.
pub fn cross_references(book: &str, chapter: u32) -> Result<ChapterReferences, RemoteError> {
    let book = safe(book);
    let Some(body) = cached(
        &format!("{API_BASE}/api/d/open-cross-ref/{book}/{chapter}.json"),
        &format!("open-cross-ref/{book}/{chapter}.json"),
    )?
    else {
        return Ok(ChapterReferences::new());
    };
    let file: ReferencesFile = serde_json::from_slice(&body)?;
    Ok(file
        .chapter
        .content
        .into_iter()
        .map(|mut verse| {
            verse
                .references
                .sort_by_key(|reference| std::cmp::Reverse(reference.score));
            (verse.verse, verse.references)
        })
        .collect())
}

/// A commentary the API offers.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Commentary {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default, rename = "englishName")]
    pub english_name: String,
}

impl Commentary {
    pub fn title(&self) -> &str {
        if self.name.trim().is_empty() {
            &self.english_name
        } else {
            &self.name
        }
    }
}

#[derive(Deserialize)]
struct CommentaryList {
    commentaries: Vec<Commentary>,
}

/// Every commentary the API offers.
pub fn commentaries() -> Result<Vec<Commentary>, RemoteError> {
    let body = cached(
        &format!("{API_BASE}/api/available_commentaries.json"),
        "commentaries.json",
    )?
    .unwrap_or_default();
    let list: CommentaryList = serde_json::from_slice(&body)?;
    Ok(list.commentaries)
}

/// One chapter of a commentary. Commentaries often discuss a run of verses
/// at once, so each section starts at a verse and runs to the next one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommentaryChapter {
    pub introduction: String,
    /// `(first verse, text)`, in verse order.
    pub sections: Vec<(u32, String)>,
}

/// The commentary section covering a verse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommentarySection<'a> {
    pub start: u32,
    /// Last verse the section covers, when another section follows.
    pub end: Option<u32>,
    pub text: &'a str,
}

impl CommentaryChapter {
    pub fn for_verse(&self, verse: u32) -> Option<CommentarySection<'_>> {
        let position = self
            .sections
            .iter()
            .rposition(|(start, _)| *start <= verse)?;
        let (start, text) = &self.sections[position];
        let end = self
            .sections
            .get(position + 1)
            .map(|(next, _)| next.saturating_sub(1))
            .filter(|end| end > start);
        Some(CommentarySection {
            start: *start,
            end,
            text,
        })
    }
}

#[derive(Deserialize)]
struct CommentaryFile {
    chapter: CommentaryContent,
}

#[derive(Deserialize)]
struct CommentaryContent {
    #[serde(default)]
    introduction: Option<String>,
    #[serde(default)]
    content: Vec<Value>,
}

/// A commentary chapter, or `None` when the commentary skips that book.
pub fn commentary_chapter(
    commentary: &str,
    book: &str,
    chapter: u32,
) -> Result<Option<CommentaryChapter>, RemoteError> {
    let (commentary, book) = (safe(commentary), safe(book));
    let Some(body) = cached(
        &format!("{API_BASE}/api/c/{commentary}/{book}/{chapter}.json"),
        &format!("c/{commentary}/{book}/{chapter}.json"),
    )?
    else {
        return Ok(None);
    };
    let file: CommentaryFile = serde_json::from_slice(&body)?;
    let mut sections: Vec<(u32, String)> = file
        .chapter
        .content
        .iter()
        .filter(|item| item.get("type").and_then(Value::as_str) == Some("verse"))
        .filter_map(|item| {
            let number = item.get("number")?.as_u64()? as u32;
            let text = flatten(item.get("content")?);
            (!text.is_empty()).then_some((number, text))
        })
        .collect();
    sections.sort_by_key(|(number, _)| *number);
    Ok(Some(CommentaryChapter {
        introduction: file
            .chapter
            .introduction
            .unwrap_or_default()
            .trim()
            .to_string(),
        sections,
    }))
}

/// Verse content items (strings, `{text}`, `{heading}`, `{lineBreak}`,
/// `{noteId}`) as plain text with line breaks.
fn flatten(content: &Value) -> String {
    let mut out = String::new();
    let Some(items) = content.as_array() else {
        return content.as_str().unwrap_or_default().trim().to_string();
    };
    for item in items {
        if let Some(text) = item.as_str() {
            out.push_str(text);
        } else if let Some(text) = item.get("text").and_then(Value::as_str) {
            out.push_str(text);
        } else if let Some(heading) = item.get("heading").and_then(Value::as_str) {
            out.push_str("\n\n");
            out.push_str(heading);
            out.push('\n');
        } else if item.get("lineBreak").is_some() {
            out.push('\n');
        }
    }
    out.trim().to_string()
}

/// Reads `relative` from the cache, or downloads `url` and stores it.
/// A 404 means the API has nothing there and returns `None`.
pub(crate) fn cached(url: &str, relative: &str) -> Result<Option<Vec<u8>>, RemoteError> {
    // Built into the app (KJV study data): no disk or network needed.
    if let Some(body) = crate::bundle::get(relative) {
        return Ok(Some(body));
    }
    let path = cache_path(relative);
    if let Ok(body) = fs::read(&path) {
        return Ok(Some(body));
    }
    let Some(body) = http_get(url)? else {
        return Ok(None);
    };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let temporary = path.with_extension("json.tmp");
    if fs::write(&temporary, &body).is_ok() {
        let _ = fs::rename(&temporary, &path);
    }
    Ok(Some(body))
}

fn cache_path(relative: &str) -> PathBuf {
    crate::cache_dir().join("api").join(relative)
}

/// Keep ids to characters that are safe in a URL path and a file name.
pub(crate) fn safe(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_verse_falls_in_the_section_that_starts_before_it() {
        let chapter = CommentaryChapter {
            introduction: String::new(),
            sections: vec![(1, "one".into()), (22, "two".into())],
        };

        let section = chapter.for_verse(16).unwrap();
        assert_eq!(
            (section.start, section.end, section.text),
            (1, Some(21), "one")
        );
        let last = chapter.for_verse(30).unwrap();
        assert_eq!((last.start, last.end), (22, None));
    }

    #[test]
    fn commentary_content_flattens_to_text() {
        let value: Value = serde_json::from_str(
            r#"["First.", {"lineBreak": true}, {"text": "Second."}, {"noteId": 1}]"#,
        )
        .unwrap();
        assert_eq!(flatten(&value), "First.\nSecond.");
    }

    #[test]
    fn cross_references_parse_and_keep_ranges() {
        let file: ReferencesFile = serde_json::from_str(
            r#"{"chapter":{"number":3,"content":[{"verse":16,"references":[
                {"book":"ROM","chapter":5,"verse":8,"score":935},
                {"book":"1JN","chapter":4,"verse":9,"endVerse":10,"score":663}]}]}}"#,
        )
        .unwrap();
        let refs = &file.chapter.content[0].references;
        assert_eq!(refs[1].end_verse, Some(10));
        assert_eq!(refs[0].book, "ROM");
    }
}
