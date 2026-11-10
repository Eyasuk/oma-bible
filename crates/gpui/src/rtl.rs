//! Right-to-left paragraphs (Arabic, Hebrew, Persian, Urdu, Syriac…).
//!
//! GPUI shapes a whole paragraph as one visual line and then breaks it at
//! glyph x positions. For right-to-left text the leftmost glyph is the *end*
//! of the sentence, so the built-in wrapping would put the last words on the
//! first line. This element wraps by words in reading order instead, then
//! shapes each line on its own (the text system still joins letters and
//! orders mixed runs within a line) and lines it up against the right edge.

use std::{cell::RefCell, collections::HashMap, rc::Rc};

use gpui_kit::*;
use oma_bible_core::direction::{Direction, direction_of};

use crate::marker;
use crate::words::{self, WordHits};

/// A felt-tip highlight drawn under each line, as in [`crate::marker`].
#[derive(Clone, Copy)]
pub struct Highlight {
    pub color: Hsla,
    pub alpha: f32,
    pub seed: u32,
}

/// A right-aligned, right-to-left paragraph. It takes its font, size,
/// colour and line height from the surrounding text style.
pub struct RtlText {
    text: SharedString,
    highlight: Option<Highlight>,
    max_lines: Option<usize>,
    hits: Option<WordHits>,
    state: Rc<RefCell<Layout>>,
}

/// Wrapped lines from earlier frames. The element is rebuilt every frame,
/// so without this each redraw measured every word of every RTL verse again.
#[derive(Clone, PartialEq, Eq, Hash)]
struct WrapKey {
    text: SharedString,
    width: Option<u32>,
    font_size: u32,
    family: SharedString,
    weight: u32,
    max_lines: Option<usize>,
}

/// Enough for several chapters at a few widths; cleared when full.
const WRAP_CACHE_LIMIT: usize = 4096;

/// The wrapped lines of a paragraph and the size they take.
type Wrapped = (Vec<SharedString>, Size<Pixels>);

thread_local! {
    static WRAP_CACHE: RefCell<HashMap<WrapKey, Wrapped>> = RefCell::new(HashMap::new());
}

#[derive(Default)]
struct Layout {
    lines: Vec<SharedString>,
    wrap_width: Option<Pixels>,
    size: Option<Size<Pixels>>,
}

impl RtlText {
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self {
            text: text.into(),
            highlight: None,
            max_lines: None,
            hits: None,
            state: Rc::default(),
        }
    }

    pub fn highlight(mut self, highlight: Option<Highlight>) -> Self {
        self.highlight = highlight;
        self
    }

    /// Records the laid-out lines so a click can find its word.
    pub fn word_hits(mut self, hits: WordHits) -> Self {
        self.hits = Some(hits);
        self
    }

    /// Shows at most `lines` lines, ending the last one with an ellipsis.
    pub fn max_lines(mut self, lines: usize) -> Self {
        self.max_lines = Some(lines.max(1));
        self
    }
}

/// Plain text that turns into an [`RtlText`] when it reads right to left,
/// for places that show text from any translation.
pub fn paragraph(text: impl Into<SharedString>, max_lines: Option<usize>) -> AnyElement {
    let text = text.into();
    match direction_of(&text) {
        Direction::Rtl => {
            let mut element = RtlText::new(text);
            if let Some(lines) = max_lines {
                element = element.max_lines(lines);
            }
            div().w_full().child(element).into_any_element()
        }
        Direction::Ltr => match max_lines {
            Some(lines) => div().line_clamp(lines).child(text).into_any_element(),
            None => div().child(text).into_any_element(),
        },
    }
}

impl IntoElement for RtlText {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for RtlText {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        _cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let style = window.text_style();
        let font_size = style.font_size.to_pixels(window.rem_size());
        let line_height = style.line_height_in_pixels(window.rem_size());
        let run = style.to_run(0);
        let text = self.text.clone();
        let max_lines = self.max_lines;
        let state = self.state.clone();

        let mut layout_style = Style::default();
        layout_style.size.width = relative(1.).into();
        let layout_id =
            window.request_measured_layout(layout_style, move |known, available, window, _cx| {
                let wrap_width = known.width.or(match available.width {
                    AvailableSpace::Definite(width) => Some(width),
                    AvailableSpace::MinContent => Some(Pixels::ZERO),
                    AvailableSpace::MaxContent => None,
                });
                if let Some(size) = state.borrow().size
                    && state.borrow().wrap_width == wrap_width
                {
                    return size;
                }
                let key = WrapKey {
                    text: text.clone(),
                    width: wrap_width.map(|width| f32::from(width).to_bits()),
                    font_size: f32::from(font_size).to_bits(),
                    family: run.font.family.clone(),
                    weight: run.font.weight.0.to_bits(),
                    max_lines,
                };
                if let Some((lines, size)) =
                    WRAP_CACHE.with(|cache| cache.borrow().get(&key).cloned())
                {
                    *state.borrow_mut() = Layout {
                        lines,
                        wrap_width,
                        size: Some(size),
                    };
                    return size;
                }

                let measure = |text: &str, window: &mut Window| {
                    let run = TextRun {
                        len: text.len(),
                        ..run.clone()
                    };
                    window
                        .text_system()
                        .shape_line(
                            SharedString::from(text.to_string()),
                            font_size,
                            &[run],
                            None,
                        )
                        .width()
                };
                let mut lines = wrap(&text, wrap_width, |part| measure(part, window));
                if let Some(max) = max_lines
                    && lines.len() > max
                {
                    lines.truncate(max);
                    if let Some(last) = lines.last_mut() {
                        last.push_str(" …");
                    }
                }
                let width = match wrap_width {
                    Some(width) if width > Pixels::ZERO => width,
                    _ => lines
                        .iter()
                        .map(|line| measure(line, window))
                        .fold(Pixels::ZERO, Pixels::max),
                };
                let size = size(width, line_height * lines.len() as f32);
                let lines: Vec<SharedString> = lines.into_iter().map(SharedString::from).collect();
                WRAP_CACHE.with(|cache| {
                    let mut cache = cache.borrow_mut();
                    if cache.len() >= WRAP_CACHE_LIMIT {
                        cache.clear();
                    }
                    cache.insert(key, (lines.clone(), size));
                });
                *state.borrow_mut() = Layout {
                    lines,
                    wrap_width,
                    size: Some(size),
                };
                size
            });
        (layout_id, ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _window: &mut Window,
        _cx: &mut App,
    ) {
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let style = window.text_style();
        let font_size = style.font_size.to_pixels(window.rem_size());
        let line_height = style.line_height_in_pixels(window.rem_size());
        let state = self.state.borrow();
        let mut shaped_lines = Vec::with_capacity(state.lines.len());

        for (row, line) in state.lines.iter().enumerate() {
            let shaped = window.text_system().shape_line(
                line.clone(),
                font_size,
                &[style.to_run(line.len())],
                None,
            );
            let top = bounds.top() + line_height * row as f32;
            let left = bounds.right() - shaped.width();
            if let Some(highlight) = self.highlight {
                marker::stroke_line(
                    f32::from(left),
                    f32::from(bounds.right()),
                    top,
                    line_height,
                    font_size,
                    highlight.seed.wrapping_mul(97).wrapping_add(row as u32),
                    highlight.color,
                    highlight.alpha,
                    window,
                );
            }
            // A line that fails to paint is skipped; the rest still draw.
            let _ = shaped.paint(
                point(left, top),
                line_height,
                TextAlign::Left,
                None,
                window,
                cx,
            );
            shaped_lines.push(shaped);
        }
        if let Some(hits) = &self.hits {
            *hits.borrow_mut() = Some(words::Layout::Rtl {
                bounds,
                line_height,
                lines: shaped_lines,
            });
        }
    }
}

/// Greedy word wrap in reading order. `measure` gives a run's width;
/// `None` means no width limit. A word wider than the line gets a line of
/// its own rather than being split mid-word (that would break joined
/// letters apart).
fn wrap(text: &str, width: Option<Pixels>, mut measure: impl FnMut(&str) -> Pixels) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let words: Vec<&str> = paragraph.split_whitespace().collect();
        let Some(width) = width else {
            lines.push(words.join(" "));
            continue;
        };
        let space = measure(" ");
        let mut line = String::new();
        let mut line_width = Pixels::ZERO;
        for word in words {
            let word_width = measure(word);
            if line.is_empty() {
                line.push_str(word);
                line_width = word_width;
            } else if line_width + space + word_width <= width {
                line.push(' ');
                line.push_str(word);
                line_width += space + word_width;
            } else {
                lines.push(std::mem::take(&mut line));
                line.push_str(word);
                line_width = word_width;
            }
        }
        lines.push(line);
    }
    lines
}

#[cfg(test)]
mod tests {
    // Not `super::*`: GPUI's own `test` macro would shadow the std one.
    use super::wrap;
    use gpui_kit::{Pixels, px};

    fn by_chars(text: &str) -> Pixels {
        px(text.chars().count() as f32)
    }

    #[test]
    fn wraps_in_reading_order() {
        let lines = wrap("aaa bbb ccc ddd", Some(px(7.)), by_chars);
        assert_eq!(lines, ["aaa bbb", "ccc ddd"]);
    }

    #[test]
    fn long_words_get_their_own_line() {
        let lines = wrap("a bbbbbbbb c", Some(px(4.)), by_chars);
        assert_eq!(lines, ["a", "bbbbbbbb", "c"]);
    }

    #[test]
    fn no_width_means_one_line() {
        assert_eq!(wrap("a  b c", None, by_chars), ["a b c"]);
    }
}
