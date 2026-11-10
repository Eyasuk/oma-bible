//! Which word of a verse sits under the pointer.
//!
//! The verse text element does not say where its words are, so the reader
//! keeps a [`WordHits`] per verse. A transparent canvas lays the verse out
//! the same way the text does (same style and width, so the line layout
//! cache hands back the same lines) and records it; [`crate::rtl::RtlText`]
//! records its own lines. A double-click then looks the word up here.

use std::{cell::RefCell, rc::Rc};

use gpui_kit::*;

/// The laid-out verse, filled in while painting.
pub type WordHits = Rc<RefCell<Option<Layout>>>;

pub enum Layout {
    /// Left-to-right text wrapped by GPUI.
    Wrapped {
        bounds: Bounds<Pixels>,
        line_height: Pixels,
        lines: Vec<WrappedLine>,
    },
    /// Right-to-left lines, each set against the right edge.
    Rtl {
        bounds: Bounds<Pixels>,
        line_height: Pixels,
        lines: Vec<ShapedLine>,
    },
}

/// A word found under the pointer: which word of the verse it is (counting
/// whitespace-separated words from 0) and its text.
pub struct Hit {
    pub index: usize,
    pub text: String,
}

/// A canvas that fills its parent and records how `text` is laid out.
/// Put it inside a `relative()` wrapper that carries the text's size and
/// line height, next to the text itself.
pub fn recorder(text: SharedString, hits: WordHits) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, (), window, _cx| {
            let style = window.text_style();
            let font_size = style.font_size.to_pixels(window.rem_size());
            let line_height = style.line_height_in_pixels(window.rem_size());
            let run = style.to_run(text.len());
            if let Ok(lines) = window.text_system().shape_text(
                text.clone(),
                font_size,
                &[run],
                Some(bounds.size.width),
                None,
            ) {
                *hits.borrow_mut() = Some(Layout::Wrapped {
                    bounds,
                    line_height,
                    lines: lines.into_vec(),
                });
            }
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

/// The word under `position`, if any.
pub fn word_at(hits: &WordHits, position: Point<Pixels>) -> Option<Hit> {
    let layout = hits.borrow();
    match layout.as_ref()? {
        Layout::Wrapped {
            bounds,
            line_height,
            lines,
        } => {
            if !bounds.contains(&position) {
                return None;
            }
            let mut top = bounds.top();
            let mut words_before = 0;
            for line in lines {
                let height = line.size(*line_height).height;
                if position.y < top + height {
                    let local = point(position.x - bounds.left(), position.y - top);
                    let index = line.index_for_position(local, *line_height).ok()?;
                    return word_in(&line.text, index, words_before);
                }
                words_before += line.text.split_whitespace().count();
                top += height;
            }
            None
        }
        Layout::Rtl {
            bounds,
            line_height,
            lines,
        } => {
            if !bounds.contains(&position) {
                return None;
            }
            let row = ((position.y - bounds.top()) / *line_height) as usize;
            let line = lines.get(row)?;
            let left = bounds.right() - line.width();
            if position.x < left {
                return None;
            }
            let index = line.index_for_x(position.x - left)?;
            let words_before = lines[..row]
                .iter()
                .map(|line| line.text.split_whitespace().count())
                .sum();
            word_in(&line.text, index, words_before)
        }
    }
}

/// The whitespace-separated word of `text` that holds byte `index`.
fn word_in(text: &str, index: usize, words_before: usize) -> Option<Hit> {
    let mut start = None;
    let mut ordinal = words_before;
    for (offset, c) in text.char_indices().chain([(text.len(), ' ')]) {
        match (c.is_whitespace(), start) {
            (false, None) => start = Some(offset),
            (true, Some(from)) => {
                if (from..offset).contains(&index) {
                    return Some(Hit {
                        index: ordinal,
                        text: text[from..offset].to_string(),
                    });
                }
                ordinal += 1;
                start = None;
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::word_in;

    #[test]
    fn finds_the_word_around_an_index() {
        let text = "In the beginning God";
        let hit = word_in(text, 8, 0).unwrap();
        assert_eq!((hit.index, hit.text.as_str()), (2, "beginning"));
        let hit = word_in(text, 0, 5).unwrap();
        assert_eq!((hit.index, hit.text.as_str()), (5, "In"));
        assert!(word_in(text, 2, 0).is_none());
    }
}
