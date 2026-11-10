//! Right-to-left text for terminals.
//!
//! A terminal draws characters left to right in the order it receives them,
//! and most terminals (Alacritty, Ghostty, foot, kitty) neither reorder
//! right-to-left runs nor join Arabic letters. So each wrapped line is turned
//! into what should appear on screen: Arabic letters are swapped for their
//! joined presentation forms, and runs are reordered with the Unicode bidi
//! algorithm, keeping each letter's vowel marks after it.
//!
//! Vowel points and other combining marks (Arabic harakat, Hebrew niqqud)
//! are dropped: terminal fonts rarely carry them, and in foot and others a
//! letter with a mark on it is drawn as the bare mark. The text stays
//! readable without them, as most printed Arabic and modern Hebrew is.
//!
//! Terminals that do their own bidi (VTE, Konsole, mlterm) would undo the
//! reordering; setting `OMA_BIBLE_TERMINAL_BIDI=1` leaves the text in
//! reading order. `OMA_BIBLE_TERMINAL_MARKS=1` keeps the vowel marks.

use std::borrow::Cow;

use oma_bible_core::direction::{Direction, direction_of, has_rtl, is_rtl_char};
use unicode_bidi::{BidiInfo, Level};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

fn env_flag(name: &str) -> bool {
    std::env::var_os(name).is_some_and(|value| value != "0" && !value.is_empty())
}

/// `text` (one line, in reading order) as it should be written to the
/// terminal. Left-to-right text without any RTL letters is returned as is.
pub fn visual(text: &str) -> Cow<'_, str> {
    if !has_rtl(text) {
        return Cow::Borrowed(text);
    }
    let text: Cow<'_, str> = if env_flag("OMA_BIBLE_TERMINAL_MARKS") {
        Cow::Borrowed(text)
    } else {
        Cow::Owned(strip_marks(text))
    };
    let shaped = shape_arabic(&text);
    if env_flag("OMA_BIBLE_TERMINAL_BIDI") {
        return Cow::Owned(shaped);
    }
    let base = match direction_of(&shaped) {
        Direction::Rtl => Level::rtl(),
        Direction::Ltr => Level::ltr(),
    };
    Cow::Owned(reorder(&shaped, base))
}

/// Removes combining marks that belong to right-to-left scripts.
fn strip_marks(text: &str) -> String {
    text.chars()
        .filter(|&c| !(is_mark(c) && is_rtl_char(c)))
        .collect()
}

/// The display width of `text` in terminal cells.
pub fn width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

fn reorder(text: &str, base: Level) -> String {
    let info = BidiInfo::new(text, Some(base));
    let mut out = String::with_capacity(text.len());
    for para in &info.paragraphs {
        let line = para.range.clone();
        let (levels, runs) = info.visual_runs(para, line);
        for run in runs {
            let slice = &text[run.clone()];
            if levels[run.start].is_rtl() {
                push_reversed(slice, &mut out);
            } else {
                out.push_str(slice);
            }
        }
    }
    out
}

/// Appends `run` backwards by character cluster (a letter and the marks
/// that follow it), mirroring brackets, so vowel points stay on their letter.
fn push_reversed(run: &str, out: &mut String) {
    let mut clusters: Vec<&str> = Vec::new();
    let mut start = 0;
    for (index, c) in run.char_indices() {
        if index > start && !is_mark(c) {
            clusters.push(&run[start..index]);
            start = index;
        }
    }
    if start < run.len() {
        clusters.push(&run[start..]);
    }
    for cluster in clusters.into_iter().rev() {
        let mut chars = cluster.chars();
        if let Some(first) = chars.next() {
            out.push(mirror(first));
            out.extend(chars);
        }
    }
}

/// Combining marks and other zero-width characters ride on the letter
/// before them.
fn is_mark(c: char) -> bool {
    c.width() == Some(0) && !c.is_whitespace() && !c.is_control()
}

fn mirror(c: char) -> char {
    match c {
        '(' => ')',
        ')' => '(',
        '[' => ']',
        ']' => '[',
        '{' => '}',
        '}' => '{',
        '<' => '>',
        '>' => '<',
        '«' => '»',
        '»' => '«',
        '‹' => '›',
        '›' => '‹',
        _ => c,
    }
}

/// The presentation forms of an Arabic letter: isolated, final, and for
/// letters that join on both sides, initial and medial.
struct Forms {
    isolated: char,
    last: char,
    first: Option<char>,
    middle: Option<char>,
}

fn forms(c: char) -> Option<Forms> {
    let right = |isolated: u32| Forms {
        isolated: char::from_u32(isolated).unwrap_or(c),
        last: char::from_u32(isolated + 1).unwrap_or(c),
        first: None,
        middle: None,
    };
    let dual = |isolated: u32| Forms {
        isolated: char::from_u32(isolated).unwrap_or(c),
        last: char::from_u32(isolated + 1).unwrap_or(c),
        first: char::from_u32(isolated + 2),
        middle: char::from_u32(isolated + 3),
    };
    Some(match c {
        'آ' => right(0xFE81),
        'أ' => right(0xFE83),
        'ؤ' => right(0xFE85),
        'إ' => right(0xFE87),
        'ئ' => dual(0xFE89),
        'ا' => right(0xFE8D),
        'ب' => dual(0xFE8F),
        'ة' => right(0xFE93),
        'ت' => dual(0xFE95),
        'ث' => dual(0xFE99),
        'ج' => dual(0xFE9D),
        'ح' => dual(0xFEA1),
        'خ' => dual(0xFEA5),
        'د' => right(0xFEA9),
        'ذ' => right(0xFEAB),
        'ر' => right(0xFEAD),
        'ز' => right(0xFEAF),
        'س' => dual(0xFEB1),
        'ش' => dual(0xFEB5),
        'ص' => dual(0xFEB9),
        'ض' => dual(0xFEBD),
        'ط' => dual(0xFEC1),
        'ظ' => dual(0xFEC5),
        'ع' => dual(0xFEC9),
        'غ' => dual(0xFECD),
        'ف' => dual(0xFED1),
        'ق' => dual(0xFED5),
        'ك' => dual(0xFED9),
        'ل' => dual(0xFEDD),
        'م' => dual(0xFEE1),
        'ن' => dual(0xFEE5),
        'ه' => dual(0xFEE9),
        'و' => right(0xFEED),
        'ى' => right(0xFEEF),
        'ي' => dual(0xFEF1),
        // Persian and Urdu letters (Arabic Presentation Forms-A).
        'ٱ' => right(0xFB50),
        'پ' => dual(0xFB56),
        'ٹ' => dual(0xFB66),
        'چ' => dual(0xFB7A),
        'ڈ' => right(0xFB88),
        'ژ' => right(0xFB8A),
        'ڑ' => right(0xFB8C),
        'ک' => dual(0xFB8E),
        'گ' => dual(0xFB92),
        'ں' => right(0xFB9E),
        'ۀ' => right(0xFBA4),
        'ہ' => dual(0xFBA6),
        'ھ' => dual(0xFBAA),
        'ے' => right(0xFBAE),
        'ی' => dual(0xFBFC),
        _ => return None,
    })
}

/// Tatweel (the joining stroke) has no forms but joins on both sides.
const TATWEEL: char = '\u{0640}';

/// Whether `c` connects to the letter after it.
fn joins_forward(c: char) -> bool {
    c == TATWEEL || forms(c).is_some_and(|forms| forms.first.is_some())
}

/// Whether `c` connects to the letter before it.
fn joins_backward(c: char) -> bool {
    c == TATWEEL || forms(c).is_some()
}

/// The lam-alef ligature for an alef after lam: isolated and final forms.
fn lam_alef(alef: char) -> Option<(char, char)> {
    match alef {
        'آ' => Some(('\u{FEF5}', '\u{FEF6}')),
        'أ' => Some(('\u{FEF7}', '\u{FEF8}')),
        'إ' => Some(('\u{FEF9}', '\u{FEFA}')),
        'ا' => Some(('\u{FEFB}', '\u{FEFC}')),
        _ => None,
    }
}

/// Replaces Arabic letters with the forms that join them to their
/// neighbours, which terminals do not do on their own. Vowel marks are
/// skipped over when looking for neighbours.
fn shape_arabic(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let neighbour = |from: usize, step: isize| -> Option<char> {
        let mut index = from as isize + step;
        while index >= 0 && (index as usize) < chars.len() {
            let c = chars[index as usize];
            if !is_mark(c) {
                return Some(c);
            }
            index += step;
        }
        None
    };

    let mut out = String::with_capacity(text.len());
    let mut index = 0;
    while index < chars.len() {
        let c = chars[index];
        let Some(letter) = forms(c) else {
            out.push(c);
            index += 1;
            continue;
        };
        let after_joiner = neighbour(index, -1).is_some_and(joins_forward);

        // Lam followed by alef (possibly with marks between) becomes one
        // ligature; the marks follow it.
        if c == 'ل' {
            let mut next = index + 1;
            while next < chars.len() && is_mark(chars[next]) {
                next += 1;
            }
            if let Some((isolated, last)) = chars.get(next).copied().and_then(lam_alef) {
                out.push(if after_joiner { last } else { isolated });
                out.extend(&chars[index + 1..next]);
                index = next + 1;
                continue;
            }
        }

        let before_joiner =
            letter.first.is_some() && neighbour(index, 1).is_some_and(joins_backward);
        let form = match (after_joiner, before_joiner) {
            (true, true) => letter.middle.unwrap_or(letter.last),
            (true, false) => letter.last,
            (false, true) => letter.first.unwrap_or(letter.isolated),
            (false, false) => letter.isolated,
        };
        out.push(form);
        index += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaves_left_to_right_text_alone() {
        assert!(matches!(visual("In the beginning"), Cow::Borrowed(_)));
    }

    #[test]
    fn reverses_hebrew_and_keeps_points_on_their_letter() {
        // bet + dagesh + sheva, then resh.
        let logical = "\u{05D1}\u{05BC}\u{05B0}\u{05E8}";
        let visual = reorder(logical, Level::rtl());
        assert_eq!(visual, "\u{05E8}\u{05D1}\u{05BC}\u{05B0}");
    }

    #[test]
    fn keeps_numbers_left_to_right_inside_rtl_text() {
        let visual = reorder("שלום 12 עולם", Level::rtl());
        assert_eq!(visual, "םלוע 12 םולש");
    }

    #[test]
    fn drops_vowel_marks_for_the_terminal() {
        assert_eq!(strip_marks("بَسْمِ"), "بسم");
        assert_eq!(strip_marks("בְּרֵאשִׁית"), "בראשית");
    }

    #[test]
    fn mirrors_brackets() {
        assert_eq!(reorder("(שלום)", Level::rtl()), "(םולש)");
    }

    #[test]
    fn joins_arabic_letters() {
        // بسم: initial beh, medial seen, final meem.
        assert_eq!(shape_arabic("بسم"), "\u{FE91}\u{FEB4}\u{FEE2}");
        // دار: neither dal nor alef joins the letter after it, so all three
        // stay isolated.
        assert_eq!(shape_arabic("دار"), "\u{FEA9}\u{FE8D}\u{FEAD}");
    }

    #[test]
    fn joins_across_vowel_marks() {
        // بَس: beh + fatha + seen keeps beh initial, seen final.
        assert_eq!(shape_arabic("بَس"), "\u{FE91}\u{064E}\u{FEB2}");
    }

    #[test]
    fn makes_lam_alef_ligatures() {
        // سلام: seen initial, lam-alef final (joined to seen), meem isolated.
        assert_eq!(shape_arabic("سلام"), "\u{FEB3}\u{FEFC}\u{FEE1}");
        // الأرض starts with an isolated alef, then an isolated lam-hamza-alef.
        assert!(shape_arabic("الأرض").starts_with("\u{FE8D}\u{FEF7}"));
    }
}
