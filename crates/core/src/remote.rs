//! Client for the [Free Use Bible API](https://bible.helloao.org), plus a
//! complete Amharic Bible from a second source, since the API's only Amharic
//! text is a New Testament with gaps.
//!
//! The API is plain JSON over HTTPS. Downloads are streamed in chunks so a
//! cancelled download stops as soon as the current chunk arrives, rather than
//! waiting for the complete file.

use std::{
    collections::BTreeMap,
    fmt, io,
    io::Read,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};

use serde::Deserialize;

use crate::bible::{BibleBook, Chapter, Footnote, Heading, Translation, canon};

pub const API_BASE: &str = "https://bible.helloao.org";

/// Catalogue id of the complete Amharic Bible (Old and New Testament).
pub const AMHARIC_FULL_ID: &str = "amh_full";

/// Clearer names for translations whose own names collide: both Amharic
/// texts call themselves "መጽሐፍ ቅዱስ", though `amh_amh` is only the New
/// Testament.
pub fn preferred_name(id: &str) -> Option<&'static str> {
    match id.to_ascii_lowercase().as_str() {
        "amh_full" => Some("መጽሐፍ ቅዱስ"),
        "amh_amh" => Some("አዲስ ኪዳን"),
        _ => None,
    }
}

/// The complete Amharic Bible as one JSON file, converted from
/// wordproject.org by the `magna25/amharic-bible-json` project (MIT licence).
pub const AMHARIC_FULL_URL: &str =
    "https://raw.githubusercontent.com/magna25/amharic-bible-json/main/amharic_bible.json";

/// A translation listed by the API, before it has been downloaded.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct RemoteTranslation {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default, rename = "englishName")]
    pub english_name: String,
    #[serde(default)]
    pub language: String,
    #[serde(default, rename = "languageName")]
    pub language_name: String,
    #[serde(default, rename = "languageEnglishName")]
    pub language_english_name: String,
    #[serde(default, rename = "numberOfBooks")]
    pub number_of_books: u32,
    #[serde(default, rename = "totalNumberOfChapters")]
    pub total_number_of_chapters: u32,
    #[serde(default, rename = "totalNumberOfVerses")]
    pub total_number_of_verses: u32,
    #[serde(default, rename = "textDirection")]
    pub text_direction: String,
    #[serde(default, rename = "completeTranslationApiLink")]
    pub complete_api_link: String,
    #[serde(default, rename = "simpleCompleteTranslationApiLink")]
    pub simple_complete_api_link: String,
    #[serde(default)]
    pub website: String,
    #[serde(default, rename = "licenseUrl")]
    pub license_url: String,
}

impl RemoteTranslation {
    /// The native name, falling back to the English name.
    pub fn title(&self) -> &str {
        if self.name.trim().is_empty() {
            self.english_name.trim()
        } else {
            self.name.trim()
        }
    }

    /// A readable language name, falling back to the language code.
    pub fn language_label(&self) -> &str {
        if !self.language_english_name.trim().is_empty() {
            self.language_english_name.trim()
        } else if !self.language_name.trim().is_empty() {
            self.language_name.trim()
        } else {
            self.language.trim()
        }
    }

    /// Stable language key used by the language filter.
    pub fn language_code(&self) -> &str {
        if self.language.trim().is_empty() {
            self.language_label()
        } else {
            self.language.trim()
        }
    }

    /// The English title, or the native title when there is no English one.
    pub fn english_title(&self) -> &str {
        if self.english_name.trim().is_empty() {
            self.title()
        } else {
            self.english_name.trim()
        }
    }
}

/// One language represented in the catalogue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Language {
    pub code: String,
    pub name: String,
    pub native_name: String,
    pub count: usize,
}

/// Every translation the API offers.
#[derive(Debug, Clone, Default)]
pub struct RemoteCatalog {
    translations: Vec<RemoteTranslation>,
}

impl RemoteCatalog {
    /// Fetches the catalogue of available translations.
    pub fn fetch() -> Result<Self, RemoteError> {
        let url = format!("{API_BASE}/api/available_translations.json");
        let response = client()?.get(&url).send()?;
        let response = check_status(response, &url)?;
        let available: AvailableTranslations = response.json()?;
        let mut translations = available.translations;
        translations.extend(extra_translations());
        Ok(Self::new(translations))
    }

    /// Builds a catalogue from the JSON body, useful for tests and caches.
    pub fn from_json(contents: &str) -> Result<Self, RemoteError> {
        let available: AvailableTranslations = serde_json::from_str(contents)?;
        Ok(Self::new(available.translations))
    }

    fn new(mut translations: Vec<RemoteTranslation>) -> Self {
        translations.retain(|translation| !translation.id.trim().is_empty());
        for translation in &mut translations {
            if let Some(name) = preferred_name(&translation.id) {
                translation.name = name.to_string();
            }
        }
        translations.sort_by(|left, right| {
            left.title()
                .to_lowercase()
                .cmp(&right.title().to_lowercase())
                .then_with(|| left.id.cmp(&right.id))
        });
        Self { translations }
    }

    pub fn translations(&self) -> &[RemoteTranslation] {
        &self.translations
    }

    pub fn is_empty(&self) -> bool {
        self.translations.is_empty()
    }

    pub fn find(&self, id: &str) -> Option<&RemoteTranslation> {
        self.translations
            .iter()
            .find(|translation| translation.id.eq_ignore_ascii_case(id))
    }

    /// Language groupings, sorted by English name.
    pub fn languages(&self) -> Vec<Language> {
        let mut by_code = BTreeMap::<String, (String, String, usize)>::new();

        for translation in &self.translations {
            let code = translation.language_code().to_string();
            let name = translation.language_label().to_string();
            let native = translation.language_name.clone();
            let entry = by_code
                .entry(code)
                .or_insert_with(|| (name.clone(), native.clone(), 0));
            entry.2 += 1;

            if entry.0.is_empty() && !name.is_empty() {
                entry.0 = name;
            }
            if entry.1.is_empty() && !native.is_empty() {
                entry.1 = native;
            }
        }

        let mut languages = by_code
            .into_iter()
            .map(|(code, (name, native_name, count))| {
                let display_name = if name.is_empty() { code.clone() } else { name };
                Language {
                    code,
                    name: display_name,
                    native_name,
                    count,
                }
            })
            .collect::<Vec<_>>();

        languages.sort_by(|left, right| {
            left.name
                .to_lowercase()
                .cmp(&right.name.to_lowercase())
                .then_with(|| left.code.cmp(&right.code))
        });
        languages
    }

    /// Translations matching a language code and a free-text query.
    ///
    /// The query is matched against the native name, English name, abbreviation,
    /// and both language names.
    pub fn filter(&self, language: Option<&str>, query: &str) -> Vec<&RemoteTranslation> {
        let query = query.trim().to_lowercase();

        self.translations
            .iter()
            .filter(|translation| {
                language.is_none_or(|code| translation.language_code() == code)
                    && (query.is_empty()
                        || [
                            translation.id.as_str(),
                            translation.name.as_str(),
                            translation.english_name.as_str(),
                            translation.language.as_str(),
                            translation.language_name.as_str(),
                            translation.language_english_name.as_str(),
                        ]
                        .iter()
                        .any(|value| value.to_lowercase().contains(&query)))
            })
            .collect()
    }
}

/// Cancellation and progress for one download.
#[derive(Debug, Clone, Default)]
pub struct DownloadControl {
    cancelled: Arc<AtomicBool>,
    downloaded: Arc<AtomicU64>,
}

impl DownloadControl {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    pub fn downloaded(&self) -> u64 {
        self.downloaded.load(Ordering::Relaxed)
    }

    fn add_downloaded(&self, bytes: u64) {
        self.downloaded.fetch_add(bytes, Ordering::Relaxed);
    }
}

/// Downloads a complete translation: from the Bible API in the simplified
/// chapter format, or the complete Amharic Bible from its own source.
pub fn download_translation(
    id: &str,
    control: &DownloadControl,
) -> Result<Translation, RemoteError> {
    if id.eq_ignore_ascii_case(AMHARIC_FULL_ID) {
        let contents = fetch_bytes(AMHARIC_FULL_URL, control)?;
        return parse_amharic_full(&contents);
    }
    let url = format!("{API_BASE}/api/{id}/complete.simple.json");
    let contents = fetch_bytes(&url, control)?;
    let mut translation = parse_complete(&contents)?;
    if let Some(name) = preferred_name(id) {
        translation.name = name.to_string();
    }
    Ok(translation)
}

/// Where a catalogue entry is downloaded from, for status messages.
pub fn source_name(id: &str) -> &'static str {
    if id.eq_ignore_ascii_case(AMHARIC_FULL_ID) {
        "github.com/magna25/amharic-bible-json"
    } else {
        "bible.helloao.org"
    }
}

/// Translations that do not come from the Bible API's catalogue.
fn extra_translations() -> Vec<RemoteTranslation> {
    vec![RemoteTranslation {
        id: AMHARIC_FULL_ID.to_string(),
        name: "መጽሐፍ ቅዱስ".to_string(),
        english_name: "Amharic Bible (complete Old and New Testament)".to_string(),
        language: "amh".to_string(),
        language_name: "አማርኛ".to_string(),
        language_english_name: "Amharic".to_string(),
        number_of_books: 66,
        total_number_of_chapters: 1189,
        total_number_of_verses: 31102,
        text_direction: "ltr".to_string(),
        website: "https://github.com/magna25/amharic-bible-json".to_string(),
        ..RemoteTranslation::default()
    }]
}

/// Streams a URL into memory, stopping between chunks when cancelled.
fn fetch_bytes(url: &str, control: &DownloadControl) -> Result<Vec<u8>, RemoteError> {
    let mut response = client()?.get(url).send()?;
    response = check_status(response, url)?;

    let mut contents = Vec::new();
    let mut buffer = [0u8; 64 * 1024];

    loop {
        if control.is_cancelled() {
            return Err(RemoteError::Cancelled);
        }

        match response.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => {
                contents.extend_from_slice(&buffer[..read]);
                control.add_downloaded(read as u64);
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(RemoteError::Read(error)),
        }
    }

    if control.is_cancelled() {
        return Err(RemoteError::Cancelled);
    }
    Ok(contents)
}

#[derive(Debug, Deserialize)]
struct AmharicBible {
    books: Vec<AmharicBook>,
}

#[derive(Debug, Deserialize)]
struct AmharicBook {
    title: String,
    #[serde(default)]
    chapters: Vec<AmharicChapter>,
}

#[derive(Debug, Deserialize)]
struct AmharicChapter {
    chapter: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    verses: Vec<String>,
}

/// The Amharic file lists the 66 books in canonical order. Joined verses
/// ("2-3" in print) keep their text on the last verse and mark the others
/// with `-` or `[see v3]`; those become empty verses.
fn parse_amharic_full(contents: &[u8]) -> Result<Translation, RemoteError> {
    let bible: AmharicBible = serde_json::from_slice(contents)?;
    let books = bible
        .books
        .into_iter()
        .enumerate()
        .map(|(index, book)| {
            let mut chapters = book
                .chapters
                .into_iter()
                .enumerate()
                .map(|(position, chapter)| {
                    let number = chapter
                        .chapter
                        .trim()
                        .parse()
                        .unwrap_or(position as u32 + 1);
                    let verses = chapter
                        .verses
                        .into_iter()
                        .map(|verse| {
                            let verse = verse.trim();
                            if verse == "-" || verse.starts_with("[see") {
                                String::new()
                            } else {
                                verse.to_string()
                            }
                        })
                        .collect();
                    let mut chapter_out = Chapter::new(number, verses);
                    let title = chapter.title.trim();
                    if !title.is_empty() && !title.starts_with("[see") {
                        chapter_out.headings.push(Heading {
                            verse: 1,
                            text: title.to_string(),
                        });
                    }
                    chapter_out
                })
                .collect::<Vec<_>>();
            chapters.sort_by_key(|chapter| chapter.number);
            let book_out = BibleBook::new(book.title.trim(), chapters);
            match canon::BOOKS.get(index) {
                Some((id, _)) => book_out.with_id(*id),
                None => book_out,
            }
        })
        .filter(|book| !book.chapters.is_empty())
        .collect();

    let mut translation = Translation::new("መጽሐፍ ቅዱስ", "Amharic", AMHARIC_FULL_ID, books);
    translation.joined_verses = true;
    Ok(translation)
}

/// Saves a downloaded translation under the user data directory.
pub fn save_download(translation: &Translation, id: &str) -> Result<PathBuf, RemoteError> {
    let path = crate::user_data_dir().join(format!("{}.json", safe_file_stem(id)));
    translation.save(&path)?;
    Ok(path)
}

/// `amh_amh` stays as-is; unusual characters are dropped for a safe filename.
pub fn safe_file_stem(value: &str) -> String {
    let stem = value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
        .collect::<String>();

    if stem.is_empty() {
        String::from("translation")
    } else {
        stem
    }
}

#[derive(Debug, Deserialize)]
struct AvailableTranslations {
    translations: Vec<RemoteTranslation>,
}

#[derive(Debug, Deserialize)]
struct CompleteTranslation {
    translation: RemoteTranslation,
    #[serde(default)]
    books: Vec<RemoteBook>,
}

#[derive(Debug, Deserialize)]
struct RemoteBook {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    common_name: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    order: u32,
    #[serde(default)]
    chapters: Vec<RemoteChapter>,
}

#[derive(Debug, Deserialize)]
struct RemoteChapter {
    chapter: SimpleChapter,
}

#[derive(Debug, Deserialize)]
struct SimpleChapter {
    number: u32,
    #[serde(default)]
    content: Vec<SimpleContent>,
}

#[derive(Debug, Deserialize)]
struct SimpleContent {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    number: Option<u32>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    footnotes: Vec<SimpleNote>,
    #[serde(default)]
    headings: Vec<SimpleNote>,
}

/// A verse footnote or inline heading; only the text is kept.
#[derive(Debug, Deserialize)]
struct SimpleNote {
    #[serde(default)]
    text: String,
}

fn parse_complete(contents: &[u8]) -> Result<Translation, RemoteError> {
    let complete: CompleteTranslation = serde_json::from_slice(contents)?;

    let mut books = complete
        .books
        .into_iter()
        .map(RemoteBook::into_local)
        .collect::<Vec<_>>();

    books.sort_by(|left, right| {
        left.1
            .cmp(&right.1)
            .then_with(|| left.0.name.cmp(&right.0.name))
    });
    books.retain(|(book, _)| !book.chapters.is_empty());
    let books = books.into_iter().map(|(book, _)| book).collect();

    let language = if !complete.translation.language_english_name.trim().is_empty() {
        complete.translation.language_english_name.trim()
    } else if !complete.translation.language_name.trim().is_empty() {
        complete.translation.language_name.trim()
    } else if !complete.translation.language.trim().is_empty() {
        complete.translation.language.trim()
    } else {
        "Unknown"
    };

    let name = if complete.translation.name.trim().is_empty() {
        complete.translation.english_title()
    } else {
        complete.translation.name.trim()
    };

    Ok(Translation::new(
        name,
        language,
        complete.translation.id.clone(),
        books,
    ))
}

impl RemoteBook {
    fn into_local(self) -> (BibleBook, u32) {
        let id = self.id.trim().to_string();
        let name = [self.name, self.common_name, self.title, self.id]
            .into_iter()
            .find(|value| !value.trim().is_empty())
            .unwrap_or_else(|| String::from("Unknown book"));

        let mut chapters = self
            .chapters
            .into_iter()
            .map(|wrapper| simple_chapter(wrapper.chapter))
            .collect::<Vec<_>>();
        chapters.sort_by_key(|chapter| chapter.number);

        (
            BibleBook::new(name.trim(), chapters).with_id(id),
            self.order,
        )
    }
}

fn simple_chapter(chapter: SimpleChapter) -> Chapter {
    let mut numbered = Vec::new();
    let mut headings = Vec::new();
    let mut footnotes = Vec::new();
    // Chapter-level headings belong to the verse that follows them.
    let mut pending_headings = Vec::new();

    for content in chapter.content {
        match content.kind.as_str() {
            "heading" => {
                if let Some(text) = content.text.map(|text| text.trim().to_string())
                    && !text.is_empty()
                {
                    pending_headings.push(text);
                }
            }
            "verse" => {
                let Some(number) = content.number.filter(|number| *number >= 1) else {
                    continue;
                };
                for text in pending_headings.drain(..) {
                    headings.push(Heading {
                        verse: number,
                        text,
                    });
                }
                for heading in content.headings {
                    let text = heading.text.trim();
                    if !text.is_empty() {
                        headings.push(Heading {
                            verse: number,
                            text: text.to_string(),
                        });
                    }
                }
                for note in content.footnotes {
                    let text = note.text.trim();
                    if !text.is_empty() {
                        footnotes.push(Footnote {
                            verse: number,
                            text: text.to_string(),
                        });
                    }
                }
                let text = content.text.unwrap_or_default().trim().to_string();
                numbered.push((number, text));
            }
            _ => {}
        }
    }

    let highest = numbered
        .iter()
        .map(|(number, _)| *number)
        .max()
        .unwrap_or_default();
    let mut verses = vec![String::new(); highest as usize];

    for (number, text) in numbered {
        let slot = &mut verses[number as usize - 1];
        // A verse split across paragraphs arrives in pieces; keep them all.
        if slot.is_empty() {
            *slot = text;
        } else if !text.is_empty() {
            slot.push(' ');
            slot.push_str(&text);
        }
    }

    while verses.last().is_some_and(String::is_empty) {
        verses.pop();
    }

    let mut out = Chapter::new(chapter.number, verses);
    out.headings = headings;
    out.footnotes = footnotes;
    out
}

/// GET a small JSON file. `None` when the API answers 404.
pub(crate) fn http_get(url: &str) -> Result<Option<Vec<u8>>, RemoteError> {
    let response = client()?.get(url).send()?;
    if response.status().as_u16() == 404 {
        return Ok(None);
    }
    let response = check_status(response, url)?;
    Ok(Some(response.bytes()?.to_vec()))
}

fn client() -> Result<reqwest::blocking::Client, RemoteError> {
    reqwest::blocking::Client::builder()
        .user_agent(concat!("oma-bible/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(300))
        .build()
        .map_err(RemoteError::Http)
}

fn check_status(
    response: reqwest::blocking::Response,
    url: &str,
) -> Result<reqwest::blocking::Response, RemoteError> {
    if response.status().is_success() {
        Ok(response)
    } else {
        Err(RemoteError::Status {
            status: response.status().as_u16(),
            url: url.to_string(),
        })
    }
}

/// A download or catalogue request failed.
#[derive(Debug)]
pub enum RemoteError {
    Http(reqwest::Error),
    Status { status: u16, url: String },
    Read(io::Error),
    Parse(serde_json::Error),
    Save(io::Error),
    Cancelled,
}

impl fmt::Display for RemoteError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Http(error) => write!(formatter, "could not reach the Bible API: {error}"),
            Self::Status { status, url } => {
                write!(formatter, "the Bible API returned {status} for {url}")
            }
            Self::Read(error) => write!(formatter, "could not read the download: {error}"),
            Self::Parse(error) => write!(formatter, "could not parse the download: {error}"),
            Self::Save(error) => write!(formatter, "could not save the translation: {error}"),
            Self::Cancelled => formatter.write_str("download cancelled"),
        }
    }
}

impl std::error::Error for RemoteError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Http(error) => Some(error),
            Self::Read(error) | Self::Save(error) => Some(error),
            Self::Parse(error) => Some(error),
            Self::Status { .. } | Self::Cancelled => None,
        }
    }
}

impl From<reqwest::Error> for RemoteError {
    fn from(error: reqwest::Error) -> Self {
        Self::Http(error)
    }
}

impl From<serde_json::Error> for RemoteError {
    fn from(error: serde_json::Error) -> Self {
        Self::Parse(error)
    }
}

impl From<io::Error> for RemoteError {
    fn from(error: io::Error) -> Self {
        Self::Save(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CATALOG: &str = r#"
        {
          "translations": [
            {
              "id": "KJV",
              "name": "King James Version",
              "englishName": "King James Version",
              "language": "eng",
              "languageName": "English",
              "languageEnglishName": "English",
              "numberOfBooks": 66
            },
            {
              "id": "AMH",
              "name": "መጽሐፍ ቅዱስ",
              "englishName": "Amharic Bible",
              "language": "amh",
              "languageName": "አማርኛ",
              "languageEnglishName": "Amharic",
              "numberOfBooks": 27
            },
            {
              "id": "BSB",
              "name": "Berean Standard Bible",
              "englishName": "Berean Standard Bible",
              "language": "eng",
              "languageName": "English",
              "languageEnglishName": "English",
              "numberOfBooks": 66
            }
          ]
        }
    "#;

    const COMPLETE: &str = r#"
        {
          "translation": {
            "id": "TST",
            "name": "Test Bible",
            "englishName": "Test Bible",
            "language": "eng",
            "languageName": "English",
            "languageEnglishName": "English"
          },
          "books": [
            {
              "id": "GEN",
              "name": "Genesis",
              "order": 1,
              "chapters": [
                {
                  "chapter": {
                    "number": 1,
                    "content": [
                      { "type": "heading", "text": "The Beginning" },
                      { "type": "verse", "number": 1, "text": " One " },
                      { "type": "verse", "number": 3, "text": "Three",
                        "footnotes": [{ "noteId": 1, "text": "Or third", "caller": "+" }] },
                      { "type": "verse", "number": 3, "text": "again" }
                    ]
                  }
                }
              ]
            },
            {
              "id": "EXO",
              "name": "Exodus",
              "order": 2,
              "chapters": []
            }
          ]
        }
    "#;

    #[test]
    fn groups_languages_and_counts_translations() {
        let catalog = RemoteCatalog::from_json(CATALOG).unwrap();
        let languages = catalog.languages();

        assert_eq!(catalog.translations().len(), 3);
        assert_eq!(languages.len(), 2);
        assert_eq!(languages[0].name, "Amharic");
        assert_eq!(languages[0].count, 1);
        assert_eq!(languages[1].name, "English");
        assert_eq!(languages[1].count, 2);
    }

    #[test]
    fn filters_by_language_and_query() {
        let catalog = RemoteCatalog::from_json(CATALOG).unwrap();

        assert_eq!(catalog.filter(Some("eng"), "").len(), 2);
        assert_eq!(catalog.filter(Some("amh"), "").len(), 1);
        assert_eq!(catalog.filter(None, "berean")[0].id, "BSB");
        assert_eq!(catalog.filter(None, "አማርኛ")[0].id, "AMH");
    }

    #[test]
    fn converts_the_simplified_complete_format() {
        let translation = parse_complete(COMPLETE.as_bytes()).unwrap();

        assert_eq!(translation.abbreviation, "TST");
        assert_eq!(translation.name, "Test Bible");
        assert_eq!(translation.language, "English");
        assert_eq!(translation.books.len(), 1);

        let genesis = translation.book("Genesis").unwrap();
        let chapter = genesis.chapter(1).unwrap();
        assert_eq!(chapter.verse(1), Some("One"));
        assert_eq!(chapter.verse(2), Some(""));
        assert_eq!(chapter.verse(3), Some("Three again"));
        assert_eq!(genesis.id, "GEN");
        assert_eq!(
            chapter.headings_before(1).collect::<Vec<_>>(),
            ["The Beginning"]
        );
        assert_eq!(chapter.footnotes_for(3).collect::<Vec<_>>(), ["Or third"]);
    }

    #[test]
    fn converts_the_complete_amharic_file() {
        let json = r#"{"title":"Amharic Bible","books":[
            {"title":"ኦሪት ዘፍጥረት","abbv":"","chapters":[
                {"chapter":"1","title":"","verses":["በመጀመሪያ","-","ሁለት ሦስት"]}]},
            {"title":"ኦሪት ዘጸአት","abbv":"","chapters":[
                {"chapter":"1","title":"መዝሙር","verses":["አንድ"]}]}]}"#;
        let translation = parse_amharic_full(json.as_bytes()).unwrap();

        assert!(translation.joined_verses);
        assert_eq!(translation.abbreviation, AMHARIC_FULL_ID);
        assert_eq!(translation.books[0].id, "GEN");
        assert_eq!(translation.books[1].id, "EXO");
        let chapter = translation.books[0].chapter(1).unwrap();
        assert_eq!(chapter.verse(2), Some(""));
        assert_eq!(chapter.verse(3), Some("ሁለት ሦስት"));
        let exodus = translation.books[1].chapter(1).unwrap();
        assert_eq!(exodus.headings_before(1).collect::<Vec<_>>(), ["መዝሙር"]);
    }

    #[test]
    fn cancellation_is_sticky() {
        let control = DownloadControl::new();

        assert!(!control.is_cancelled());
        control.cancel();
        assert!(control.is_cancelled());
    }

    #[test]
    fn filenames_are_reduced_to_safe_characters() {
        assert_eq!(safe_file_stem("amh_amh"), "amh_amh");
        assert_eq!(safe_file_stem("../BSB?"), "BSB");
        assert_eq!(safe_file_stem("???"), "translation");
    }
}
