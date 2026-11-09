use gpui_kit::assets::IconName;
use gpui_kit::base::SelectableText;
use gpui_kit::component::{ActiveTheme, Icon, h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::app::{NoteTarget, OmaBibleApp};
use crate::theme as app_theme;

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
        let visual = state.visual_range();
        let dark = state.themes.active().mode.is_dark();
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
                        .child(
                            div()
                                .text_size(px(13.))
                                .text_color(cx.theme().muted_foreground)
                                .child(reference.to_uppercase()),
                        ),
                )
                .children(visible_verses.into_iter().map(|index| {
                    let row_app = app.clone();
                    let menu_app = app.clone();
                    let verse = verses[index].clone();
                    let verse_number = index as u32 + 1;
                    let bookmark = state.bookmarks.find(
                        &translation_abbr,
                        &book_name,
                        chapter_number,
                        verse_number,
                    );
                    let has_note =
                        bookmark.is_some_and(|bookmark| !bookmark.note.trim().is_empty());
                    let highlight = highlights
                        .find(&translation_abbr, &book_name, chapter_number, verse_number)
                        .map(|highlight| highlight.color);
                    let is_selected = index == selected;
                    let in_visual =
                        visual.is_some_and(|(start, end)| index >= start && index <= end);
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
                        .on_click(move |_event, _window, cx| {
                            row_app.update(cx, |app, cx| {
                                app.verse_index = index;
                                cx.notify();
                            });
                        })
                        // Right-click acts on the verse under the pointer; the
                        // root view then opens the wheel around it.
                        .on_mouse_down(MouseButton::Right, move |_event, _window, cx| {
                            menu_app.update(cx, |app, _| app.verse_index = index);
                        });

                    if in_visual {
                        let fill = if is_selected {
                            cx.theme().accent.opacity(0.20)
                        } else {
                            cx.theme().selection
                        };
                        row = row.bg(fill).rounded(px(10.));
                    } else if is_selected {
                        row = row.bg(cx.theme().selection).rounded(px(10.));
                    } else if let Some(color) = highlight {
                        let alpha = if dark { 0.18 } else { 0.30 };
                        row = row
                            .bg(app_theme::highlight(color, cx).opacity(alpha))
                            .rounded(px(10.));
                    } else {
                        row = row.hover(|style| style.bg(cx.theme().muted).rounded(px(10.)));
                    }

                    if show_numbers {
                        let number_color = if is_match {
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
                                .child(verse_number.to_string()),
                        );
                    }

                    let verse_text = if verse.trim().is_empty() {
                        div()
                            .text_size(px(font_size))
                            .line_height(px(font_size * 1.62))
                            .text_color(cx.theme().muted_foreground)
                            .child("—")
                            .into_any_element()
                    } else {
                        let style = TextStyleRefinement {
                            color: Some(cx.theme().foreground),
                            font_size: Some(px(font_size).into()),
                            line_height: Some(px(font_size * 1.62).into()),
                            ..Default::default()
                        };
                        SelectableText::new(
                            SharedString::from(format!(
                                "reader-verse-text-{chapter_number}-{index}"
                            )),
                            verse,
                        )
                        .document_order(index as u64)
                        .text_style(style)
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
                                .child(Icon::new(IconName::StickyNote).size(px(14.))),
                        );
                    }

                    if let Some(color) = highlight {
                        row = row.child(
                            Icon::new(IconName::Highlighter)
                                .size(px(14.))
                                .text_color(app_theme::highlight(color, cx)),
                        );
                    }

                    row
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
