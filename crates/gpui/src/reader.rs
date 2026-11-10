use gpui_kit::assets::IconName;
use gpui_kit::base::SelectableText;
use gpui_kit::component::{ActiveTheme, Icon, h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;
use oma_bible_core::original::ClickedWord;

use crate::app::{NoteTarget, OmaBibleApp};
use crate::marker::marker;
use crate::rtl::{Highlight, RtlText};
use crate::theme as app_theme;
use crate::words::{self, WordHits};

/// The reading surface: one quiet column, large type and no chrome.
///
/// Book/chapter movement and every secondary action live in the small icon
/// bar below the text (or in the keyboard shortcuts), so the page itself is
/// only scripture.
#[derive(IntoElement)]
pub struct ReaderView {
    app: Entity<OmaBibleApp>,
}

impl ReaderView {
    pub fn new(app: Entity<OmaBibleApp>) -> Self {
        Self { app }
    }
}

impl RenderOnce for ReaderView {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let app = self.app.clone();
        let state = self.app.read(cx);

        let translation = state.translation();
        let book = state.book();
        let chapter = state.chapter();
        let book_name = book.map(|book| book.name.clone()).unwrap_or_default();
        let chapter_number = chapter.map(|chapter| chapter.number).unwrap_or(1);
        let translation_name = translation.map(|translation| translation.name.clone());
        let translation_abbr = translation
            .map(|translation| translation.abbreviation.clone())
            .unwrap_or_default();
        let verses: &[String] = chapter
            .map(|chapter| chapter.verses.as_slice())
            .unwrap_or(&[]);
        let visible_verses = state.visible_verse_indices();
        let selected = state.verse_index;
        let font_size = state.reader_font_size();
        let show_numbers = state.settings.show_verse_numbers;
        let search_query = state.search_query.trim().to_string();
        let search_matches = state.search_matches.clone();
        let highlights = &state.highlights;
        let dark = state.themes.active().mode.is_dark();
        let joined_verses = translation.is_some_and(|translation| translation.joined_verses);
        // Arabic, Hebrew and other right-to-left translations mirror the
        // row: number on the right, text wrapped and aligned from the right.
        let rtl = translation.is_some_and(|translation| translation.direction().is_rtl());
        let scroll = &state.reading_scroll;

        let reference = if book_name.is_empty() {
            "Choose a book to begin".to_string()
        } else {
            format!("{book_name} {chapter_number}")
        };

        let mut content = v_flex()
            .id("reading-scroll")
            .flex_1()
            .min_h_0()
            .w_full()
            .items_center()
            .overflow_y_scroll()
            .track_scroll(scroll)
            .pt(px(72.))
            .pb(px(80.));

        if translation_name.is_none() {
            content = content.child(
                v_flex()
                    .w_full()
                    .max_w(px(760.))
                    .px(px(40.))
                    .items_center()
                    .gap_3()
                    .child(
                        Icon::new(IconName::BookOpen)
                            .size(px(34.))
                            .text_color(cx.theme().muted_foreground),
                    )
                    .child(
                        div()
                            .text_size(px(26.))
                            .text_color(cx.theme().foreground)
                            .child("No translation installed"),
                    )
                    .child(
                        div()
                            .text_size(px(15.))
                            .text_color(cx.theme().muted_foreground)
                            .text_center()
                            .child("Press D to download one, or open the settings button below."),
                    ),
            );
        } else if verses.is_empty() {
            content = content.child(
                v_flex()
                    .w_full()
                    .max_w(px(760.))
                    .px(px(40.))
                    .items_center()
                    .gap_3()
                    .child(
                        Icon::new(IconName::BookOpen)
                            .size(px(34.))
                            .text_color(cx.theme().muted_foreground),
                    )
                    .child(
                        div()
                            .text_size(px(24.))
                            .text_color(cx.theme().foreground)
                            .child("This chapter has no verses"),
                    ),
            );
        } else {
            content = content
                .child(
                    v_flex()
                        .w_full()
                        .max_w(px(760.))
                        .px(px(40.))
                        .items_center()
                        .pb(px(52.))
                        .child(chapter_heading(&app, reference, cx)),
                )
                .children(visible_verses.into_iter().map(|index| {
                    let press_app = app.clone();
                    let drag_app = app.clone();
                    let menu_app = app.clone();
                    let verse = verses[index].clone();
                    // Where each word of the verse lands, for double-click
                    // word lookup.
                    let hits = WordHits::default();
                    let click_hits = hits.clone();
                    let word_count = verse.split_whitespace().count();
                    let verse_number = index as u32 + 1;
                    let headings: Vec<String> = chapter
                        .map(|chapter| {
                            chapter
                                .headings_before(verse_number)
                                .map(str::to_string)
                                .collect()
                        })
                        .unwrap_or_default();
                    // Joined verses ("2-3" in print) leave the first ones
                    // empty; label the verse that holds the text as a range.
                    let mut first = index;
                    while joined_verses && first > 0 && verses[first - 1].trim().is_empty() {
                        first -= 1;
                    }
                    let number_label = if first < index {
                        format!("{}–{}", first + 1, index + 1)
                    } else {
                        verse_number.to_string()
                    };
                    let bookmark = state.bookmarks.find(
                        &translation_abbr,
                        &book_name,
                        chapter_number,
                        verse_number,
                    );
                    let note_count = state
                        .notes
                        .for_verse(&translation_abbr, &book_name, chapter_number, verse_number)
                        .len();
                    let has_note = note_count > 0;
                    let highlight = highlights
                        .find(&translation_abbr, &book_name, chapter_number, verse_number)
                        .map(|highlight| highlight.color);
                    let is_selected = index == selected;
                    let in_visual = state.in_selection(index);
                    let is_match =
                        !search_query.is_empty() && search_matches.binary_search(&index).is_ok();

                    let mut row = h_flex()
                        .id(SharedString::from(format!(
                            "reader-verse-{chapter_number}-{index}"
                        )))
                        .w_full()
                        .max_w(px(760.))
                        .items_start()
                        .gap(px(14.))
                        .px(px(40.))
                        .py(px(10.))
                        .cursor_pointer()
                        .border_1()
                        .border_color(transparent_black())
                        // Press selects; drag across verses selects a range;
                        // Shift extends and Ctrl picks verses one by one.
                        // A double-click looks the word up in the original
                        // Hebrew or Greek.
                        .on_mouse_down(MouseButton::Left, move |event, _window, cx| {
                            let double = event.click_count == 2;
                            let word = double
                                .then(|| words::word_at(&click_hits, event.position))
                                .flatten()
                                .map(|hit| ClickedWord {
                                    text: hit.text,
                                    index: hit.index,
                                    count: word_count,
                                });
                            press_app.update(cx, |app, cx| {
                                app.press_verse(index, event.modifiers, cx);
                                if double {
                                    app.open_word(word, cx);
                                }
                            });
                        })
                        .on_mouse_move(move |event, _window, cx| {
                            if event.pressed_button == Some(MouseButton::Left) {
                                drag_app.update(cx, |app, cx| app.drag_over_verse(index, cx));
                            }
                        })
                        // Right-click acts on the verse under the pointer (or the
                        // selection it belongs to); the root then opens the wheel.
                        .on_mouse_down(MouseButton::Right, move |_event, _window, cx| {
                            menu_app.update(cx, |app, _| app.menu_verse(index));
                        });

                    // The cursor verse gets a short accent bar in the gutter
                    // instead of a box; a multi-verse selection is a soft tint.
                    row = row.relative().rounded(px(12.));
                    if rtl {
                        row = row.flex_row_reverse();
                    }
                    if in_visual {
                        row = row.bg(cx.theme().accent.opacity(if dark { 0.10 } else { 0.08 }));
                    } else {
                        let hover = cx.theme().muted.opacity(0.45);
                        row = row.hover(move |style| style.bg(hover));
                    }
                    if is_selected {
                        let bar = div().absolute();
                        let bar = if rtl {
                            bar.right(px(18.))
                        } else {
                            bar.left(px(18.))
                        };
                        row = row.child(
                            bar.top(px(16.))
                                .bottom(px(16.))
                                .w(px(3.))
                                .rounded_full()
                                .bg(cx.theme().accent),
                        );
                    }

                    if show_numbers {
                        let number_color = if is_match || is_selected {
                            cx.theme().accent
                        } else {
                            cx.theme().muted_foreground
                        };
                        row = row.child(
                            div()
                                .text_size(px(12.))
                                .text_color(number_color)
                                .min_w(px(22.))
                                .pt(px(8.))
                                .when(rtl, |number| number.text_right())
                                .child(number_label),
                        );
                    }

                    let verse_text = if verse.trim().is_empty() {
                        div()
                            .text_size(px(font_size))
                            .line_height(px(font_size * 1.62))
                            .text_color(cx.theme().muted_foreground)
                            .child("—")
                            .into_any_element()
                    } else if rtl {
                        div()
                            .w_full()
                            .text_size(px(font_size))
                            .line_height(px(font_size * 1.62))
                            .text_color(cx.theme().foreground)
                            .child(RtlText::new(verse).word_hits(hits).highlight(highlight.map(
                                |color| Highlight {
                                    color: app_theme::highlight(color, cx),
                                    alpha: if dark { 0.30 } else { 0.34 },
                                    seed: verse_number.wrapping_mul(2_654_435_761) ^ chapter_number,
                                },
                            )))
                            .into_any_element()
                    } else {
                        let style = TextStyleRefinement {
                            color: Some(cx.theme().foreground),
                            font_size: Some(px(font_size).into()),
                            line_height: Some(px(font_size * 1.62).into()),
                            ..Default::default()
                        };
                        let text = SelectableText::new(
                            SharedString::from(format!(
                                "reader-verse-text-{chapter_number}-{index}"
                            )),
                            verse.clone(),
                        )
                        .document_order(index as u64)
                        .text_style(style);
                        let verse = SharedString::from(verse);
                        div()
                            .relative()
                            .text_size(px(font_size))
                            .line_height(px(font_size * 1.62))
                            .child(words::recorder(verse.clone(), hits))
                            // A marker stroke under each line of words, not
                            // a box behind the verse.
                            .when_some(highlight, |wrapper, color| {
                                wrapper.child(marker(
                                    verse,
                                    app_theme::highlight(color, cx),
                                    if dark { 0.30 } else { 0.34 },
                                    verse_number.wrapping_mul(2_654_435_761) ^ chapter_number,
                                ))
                            })
                            .child(text)
                            .into_any_element()
                    };

                    row = row.child(div().flex_1().min_w_0().child(verse_text));

                    if bookmark.is_some() && !has_note {
                        row = row.child(
                            Icon::new(IconName::Bookmark)
                                .size(px(14.))
                                .text_color(cx.theme().accent),
                        );
                    }

                    if has_note {
                        let note_target = NoteTarget {
                            translation: translation_abbr.clone(),
                            book: book_name.clone(),
                            chapter: chapter_number,
                            verse: verse_number,
                        };
                        let note_app = app.clone();
                        row = row.child(
                            div()
                                .id(SharedString::from(format!(
                                    "reader-note-{chapter_number}-{index}"
                                )))
                                .cursor_pointer()
                                .text_color(cx.theme().accent)
                                .on_click(move |_event, _window, cx| {
                                    cx.stop_propagation();
                                    note_app.update(cx, |app, cx| {
                                        app.open_note_detail(note_target.clone(), cx);
                                    });
                                })
                                .flex()
                                .items_center()
                                .gap(px(2.))
                                .child(Icon::new(IconName::StickyNote).size(px(14.)))
                                .when(note_count > 1, |icon| {
                                    icon.child(
                                        div().text_size(px(10.)).child(note_count.to_string()),
                                    )
                                }),
                        );
                    }

                    if headings.is_empty() {
                        row.into_any_element()
                    } else {
                        v_flex()
                            .w_full()
                            .items_center()
                            .children(headings.into_iter().map(|heading| {
                                let indent = px(if show_numbers { 76. } else { 40. });
                                let heading_row = div().w_full().max_w(px(760.)).px(px(40.));
                                let heading_row = if rtl {
                                    heading_row.pr(indent)
                                } else {
                                    heading_row.pl(indent)
                                };
                                heading_row
                                    .pt(px(22.))
                                    .pb(px(2.))
                                    .text_size(px((font_size * 0.82).max(13.)))
                                    .line_height(px((font_size * 0.82).max(13.) * 1.5))
                                    .text_color(cx.theme().muted_foreground)
                                    .child(if rtl {
                                        RtlText::new(heading).into_any_element()
                                    } else {
                                        heading.into_any_element()
                                    })
                            }))
                            .child(row)
                            .into_any_element()
                    }
                }));
        }

        v_flex()
            .id("reader")
            .relative()
            .size_full()
            .min_h_0()
            .child(content)
            .into_any_element()
    }
}

/// The book selector: a real button naming the book and chapter, with
/// round previous/next buttons either side. The middle button opens the
/// book and chapter navigator.
fn chapter_heading(app: &Entity<OmaBibleApp>, reference: String, cx: &App) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    let surface = cx.theme().popover;
    let border = cx.theme().border;
    let hover_bg = cx.theme().muted;
    let hover_fg = cx.theme().foreground;
    let arrow = |id: &'static str, icon: IconName, app: Entity<OmaBibleApp>, forward: bool| {
        div()
            .id(id)
            .size(px(34.))
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .cursor_pointer()
            .bg(surface)
            .border_1()
            .border_color(border)
            .text_color(muted)
            .hover(move |style| style.bg(hover_bg).text_color(hover_fg))
            .on_click(move |_, _, cx| {
                app.update(cx, |app, cx| {
                    if forward {
                        app.next_chapter(cx);
                    } else {
                        app.prev_chapter(cx);
                    }
                });
            })
            .child(Icon::new(icon).size(px(14.)))
    };
    let open_app = app.clone();
    h_flex()
        .items_center()
        .gap(px(8.))
        .child(arrow(
            "chapter-prev",
            IconName::ChevronLeft,
            app.clone(),
            false,
        ))
        .child(
            h_flex()
                .id("chapter-title")
                .h(px(34.))
                .items_center()
                .gap(px(8.))
                .pl(px(14.))
                .pr(px(12.))
                .rounded_full()
                .cursor_pointer()
                .bg(surface)
                .border_1()
                .border_color(border)
                .shadow_sm()
                .text_size(px(13.))
                .text_color(cx.theme().foreground)
                .hover(move |style| style.bg(hover_bg))
                .on_click(move |_, _, cx| {
                    open_app.update(cx, |app, cx| app.open_navigator(cx));
                })
                .child(
                    Icon::new(IconName::BookOpen)
                        .size(px(14.))
                        .text_color(muted),
                )
                .child(reference)
                .child(
                    div()
                        .pl(px(6.))
                        .border_l_1()
                        .border_color(border)
                        .text_color(muted)
                        .child(Icon::new(IconName::ChevronDown).size(px(12.))),
                ),
        )
        .child(arrow(
            "chapter-next",
            IconName::ChevronRight,
            app.clone(),
            true,
        ))
}
