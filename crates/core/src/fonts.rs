//! Font availability for downloaded languages.
//!
//! Bible translations arrive in many scripts. Most desktops already have
//! fonts for the common ones, but a fresh machine can lack Ethiopic (Amharic,
//! Tigrinya, Ge'ez) entirely. When a translation is downloaded this module
//! checks the text, installs a suitable font when one is missing, and asks
//! fontconfig to prefer a narrow Ethiopic face so terminal cells line up.

use std::{fs, io, path::PathBuf, process::Command, time::Duration};

use crate::bible::Translation;

/// A static condensed Ethiopic face whose advance is close to a monospace
/// cell. The variable font at Google Fonts only exposes the regular family to
/// fontconfig, so the condensed static file is fetched instead.
const ETHIOPIC_FONT_URL: &str = "https://raw.githubusercontent.com/notofonts/notofonts.github.io/main/fonts/NotoSansEthiopic/hinted/ttf/NotoSansEthiopic-Condensed.ttf";

/// User fontconfig preference.
///
/// Noto Sans Ethiopic Condensed is about the same width as a monospace cell.
/// The regular faces are wider, which makes neighbouring letters touch in a
/// terminal. Fonts lower in the list are used when the earlier ones are not
/// installed.
const FONTCONFIG_PREFERENCE: &str = r#"<?xml version="1.0"?>
<!DOCTYPE fontconfig SYSTEM "urn:fontconfig:fonts.dtd">
<!-- Written by Oma Bible: prefer a narrow Ethiopic face in terminals. -->
<fontconfig>
  <match target="pattern">
    <edit name="family" mode="append" binding="strong">
      <string>Noto Sans Ethiopic Condensed</string>
      <string>Noto Sans Ethiopic</string>
      <string>Abyssinica SIL</string>
    </edit>
  </match>
</fontconfig>
"#;

/// A little text that represents the scripts used by a translation.
pub fn sample_text(translation: &Translation) -> String {
    let mut sample = translation.name.clone();

    if let Some(book) = translation.books.first() {
        sample.push(' ');
        sample.push_str(&book.name);
        if let Some(verse) = book
            .chapters
            .first()
            .and_then(|chapter| chapter.verses.first())
        {
            sample.push(' ');
            sample.push_str(verse);
        }
    }

    sample
}

/// Check the scripts in `text` and set up whatever fonts are missing.
///
/// `allow_install` is false during startup so the reader never waits for a
/// download; the font is fetched after the next translation download instead.
/// Returns a short message for the UI when something changed or needs the
/// user's attention.
pub fn prepare_for_text(text: &str, allow_install: bool) -> Option<String> {
    let mut notices = Vec::new();

    if contains_ethiopic(text) {
        if !font_covers('\u{1200}') {
            if allow_install {
                match install_ethiopic_font() {
                    Ok(()) => notices.push(String::from(
                        "Installed Noto Sans Ethiopic Condensed. Restart the terminal to see Amharic correctly.",
                    )),
                    Err(error) => notices.push(format!(
                        "Amharic needs an Ethiopic font, but it could not be installed: {error}"
                    )),
                }
            } else {
                notices.push(String::from(
                    "Amharic needs an Ethiopic font. It will be installed with the next translation download.",
                ));
            }
        }

        if write_fontconfig_preference().unwrap_or(false) {
            notices.push(String::from(
                "Set Ethiopic text to use a condensed font. Restart the terminal for the change to apply.",
            ));
        }
    } else if let Some(character) = text
        .chars()
        .filter(|character| character.is_alphabetic() && !character.is_ascii())
        .find(|character| !font_covers(*character))
    {
        notices.push(match script_name(character) {
            Some(script) => format!(
                "This translation uses {script} text, but no installed font covers it. Install a {script} font from your system's font packages."
            ),
            None => String::from(
                "This translation uses a script that no installed font covers. Install a font for that script from your system's font packages.",
            ),
        });
    }

    if notices.is_empty() {
        None
    } else {
        Some(notices.join("  "))
    }
}

fn contains_ethiopic(text: &str) -> bool {
    text.chars().any(is_ethiopic)
}

fn is_ethiopic(character: char) -> bool {
    matches!(
        character,
        '\u{1200}'..='\u{137F}'
            | '\u{1380}'..='\u{139F}'
            | '\u{2D80}'..='\u{2DDF}'
            | '\u{AB00}'..='\u{AB2F}'
    )
}

/// True when fontconfig can find a font for this character.
///
/// A missing `fc-list` means we cannot tell, so assume the system is fine
/// rather than nagging the reader.
fn font_covers(character: char) -> bool {
    let query = format!(":charset={:x}", character as u32);

    Command::new("fc-list")
        .arg(query)
        .output()
        .map(|output| !output.status.success() || !output.stdout.is_empty())
        .unwrap_or(true)
}

fn script_name(character: char) -> Option<&'static str> {
    match character as u32 {
        0x0370..=0x03FF | 0x1F00..=0x1FFF => Some("Greek"),
        0x0400..=0x052F => Some("Cyrillic"),
        0x0530..=0x058F => Some("Armenian"),
        0x0590..=0x05FF => Some("Hebrew"),
        0x0600..=0x06FF | 0x0750..=0x077F | 0x08A0..=0x08FF => Some("Arabic"),
        0x0900..=0x097F => Some("Devanagari"),
        0x0980..=0x09FF => Some("Bengali"),
        0x0A00..=0x0A7F => Some("Gurmukhi"),
        0x0A80..=0x0AFF => Some("Gujarati"),
        0x0B00..=0x0B7F => Some("Oriya"),
        0x0B80..=0x0BFF => Some("Tamil"),
        0x0C00..=0x0C7F => Some("Telugu"),
        0x0C80..=0x0CFF => Some("Kannada"),
        0x0D00..=0x0D7F => Some("Malayalam"),
        0x0D80..=0x0DFF => Some("Sinhala"),
        0x0E00..=0x0E7F => Some("Thai"),
        0x0E80..=0x0EFF => Some("Lao"),
        0x0F00..=0x0FFF => Some("Tibetan"),
        0x1000..=0x109F => Some("Myanmar"),
        0x10A0..=0x10FF => Some("Georgian"),
        0x1200..=0x139F | 0x2D80..=0x2DDF | 0xAB00..=0xAB2F => Some("Ethiopic"),
        0x1780..=0x17FF => Some("Khmer"),
        0x3040..=0x30FF => Some("Japanese"),
        0x4E00..=0x9FFF => Some("Chinese"),
        0xAC00..=0xD7AF => Some("Korean"),
        _ => None,
    }
}

/// Download a Noto Ethiopic face into the user's font directory.
fn install_ethiopic_font() -> Result<(), String> {
    let directory = data_home().join("fonts").join("oma-bible");
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;

    let bytes = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|error| error.to_string())?
        .get(ETHIOPIC_FONT_URL)
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .map_err(|error| error.to_string())?
        .bytes()
        .map_err(|error| error.to_string())?;

    if !is_font(&bytes) {
        return Err(String::from("the font download was not a font file"));
    }

    fs::write(directory.join("NotoSansEthiopicCondensed.ttf"), &bytes)
        .map_err(|error| error.to_string())?;
    let _ = Command::new("fc-cache").arg("-f").status();

    Ok(())
}

/// True when the bytes look like an sfnt font: TrueType, CFF, or a collection.
fn is_font(bytes: &[u8]) -> bool {
    matches!(
        bytes.get(..4),
        Some(b"\0\x01\0\0" | b"OTTO" | b"true" | b"ttcf")
    )
}

fn write_fontconfig_preference() -> io::Result<bool> {
    let directory = config_home().join("fontconfig").join("conf.d");
    let path = directory.join("60-oma-bible-ethiopic.conf");
    fs::create_dir_all(&directory)?;

    if fs::read_to_string(&path).is_ok_and(|contents| contents == FONTCONFIG_PREFERENCE) {
        return Ok(false);
    }

    fs::write(path, FONTCONFIG_PREFERENCE)?;
    Ok(true)
}

fn config_home() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .unwrap_or_else(std::env::temp_dir)
}

fn data_home() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local").join("share"))
        })
        .unwrap_or_else(std::env::temp_dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bible::{BibleBook, Chapter};

    #[test]
    fn ethiopic_text_is_recognised() {
        assert!(contains_ethiopic("መጽሐፍ ቅዱስ"));
        assert!(contains_ethiopic("ሰላም"));
        assert!(!contains_ethiopic("Genesis"));
        assert!(!contains_ethiopic("Ελληνικά"));
    }

    #[test]
    fn scripts_have_readable_names() {
        assert_eq!(script_name('Ω'), Some("Greek"));
        assert_eq!(script_name('Ж'), Some("Cyrillic"));
        assert_eq!(script_name('א'), Some("Hebrew"));
        assert_eq!(script_name('م'), Some("Arabic"));
        assert_eq!(script_name('अ'), Some("Devanagari"));
        assert_eq!(script_name('መ'), Some("Ethiopic"));
        assert_eq!(script_name('あ'), Some("Japanese"));
    }

    #[test]
    fn font_signatures_are_checked() {
        assert!(is_font(b"\0\x01\0\0rest"));
        assert!(is_font(b"OTTOrest"));
        assert!(!is_font(b"<html>"));
        assert!(!is_font(b""));
    }

    #[test]
    fn sample_includes_the_first_verse() {
        let chapter = Chapter::new(1, vec![String::from("በመጀመሪያ")]);
        let translation = Translation::new(
            "መጽሐፍ ቅዱስ",
            "Amharic",
            "amh",
            vec![BibleBook::new("የዮሐንስ ወንጌል", vec![chapter])],
        );

        let sample = sample_text(&translation);

        assert!(sample.contains("የዮሐንስ"));
        assert!(sample.contains("በመጀመሪያ"));
    }
}
