use oma_bible_core::theme::Rgb;
use oma_bible_core::user_data_dir;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Clear, List, ListItem, ListState, Paragraph},
};

use crate::app::{App, Focus, Screen};
use crate::bidi;

pub(crate) fn ui(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    let background = app.theme().background;
    frame
        .buffer_mut()
        .set_style(area, Style::new().bg(color(background)));

    match app.screen {
        Screen::Reader => render_reader(frame, area, app),
        _ => render_screen(frame, area, app),
    }

    if let Some(prompt) = &app.input {
        render_input(frame, area, app, prompt);
    }
}

fn render_reader(frame: &mut Frame, area: Rect, app: &mut App) {
    let [header_area, body_area, footer_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(area);

    render_header(frame, header_area, app);

    let [books_area, chapters_area, verses_area] = Layout::horizontal([
        Constraint::Percentage(26),
        Constraint::Length(7),
        Constraint::Min(0),
    ])
    .areas(body_area);

    render_books(frame, books_area, app);
    render_chapters(frame, chapters_area, app);
    render_verses(frame, verses_area, app);
    render_footer(frame, footer_area, app);
}

fn render_header(frame: &mut Frame, area: Rect, app: &App) {
    let theme = app.theme();
    let mut spans = vec![Span::styled(
        " Oma Bible ",
        Style::new()
            .fg(color(theme.background))
            .bg(color(theme.accent))
            .add_modifier(Modifier::BOLD),
    )];

    if let Some(translation) = app.translation() {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            bidi::visual(&translation.name).into_owned(),
            Style::new()
                .fg(color(theme.foreground))
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::raw(format!(" ({})  ", translation.abbreviation)));
        spans.push(Span::styled(
            translation.language.as_str(),
            Style::new().fg(color(theme.muted)),
        ));
    } else {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            "No translations installed · Press D to add one",
            Style::new().fg(color(theme.muted)),
        ));
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn render_books(frame: &mut Frame, area: Rect, app: &App) {
    let theme = app.theme();
    let items = app
        .translation()
        .map(|translation| {
            translation
                .books
                .iter()
                .map(|book| ListItem::new(bidi::visual(&book.name).into_owned()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let list = List::new(items)
        .block(panel_block(theme, " Books ", app.focus == Focus::Books))
        .highlight_style(selection_style(theme, app.focus == Focus::Books))
        .highlight_symbol("> ");

    let mut state = ListState::default().with_selected(Some(app.book_index));
    frame.render_stateful_widget(list, area, &mut state);
}

fn render_chapters(frame: &mut Frame, area: Rect, app: &App) {
    let theme = app.theme();
    let items = app
        .book()
        .map(|book| {
            book.chapters
                .iter()
                .map(|chapter| ListItem::new(chapter.number.to_string()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let list = List::new(items)
        .block(panel_block(theme, " Ch ", app.focus == Focus::Chapters))
        .highlight_style(selection_style(theme, app.focus == Focus::Chapters))
        .highlight_symbol("> ");

    let mut state = ListState::default().with_selected(Some(app.chapter_index));
    frame.render_stateful_widget(list, area, &mut state);
}

fn render_verses(frame: &mut Frame, area: Rect, app: &mut App) {
    let theme = app.theme();
    let block = panel_block(theme, verse_title(app), app.focus == Focus::Verses);
    let inner = block.inner(area);
    let mut lines = Vec::new();
    let mut verse_starts = Vec::new();
    let show_numbers = app.settings.show_verse_numbers;
    // Right-to-left translations are wrapped in reading order, then each
    // line is reordered for the terminal and set against the right edge
    // with the verse number on the right.
    let rtl = app
        .translation()
        .is_some_and(|translation| translation.direction().is_rtl());

    if let Some(chapter) = app.chapter() {
        if chapter.verse_count() == 0 {
            lines.push(Line::from(Span::styled(
                "This chapter has no verses.",
                Style::new().fg(color(theme.muted)),
            )));
        }

        for index in 0..chapter.verse_count() {
            let Some(verse) = chapter.verse(index as u32 + 1) else {
                continue;
            };

            let selected = index == app.verse_index;
            let matched = app.search_matches.contains(&index);
            let verse_style = if selected {
                Style::new()
                    .fg(color(theme.bright_foreground))
                    .bg(color(theme.selection))
                    .add_modifier(Modifier::BOLD)
            } else if matched {
                Style::new()
                    .fg(color(theme.foreground))
                    .bg(color(theme.lighter_background))
            } else {
                Style::new().fg(color(theme.foreground))
            };
            let prefix = if show_numbers && rtl {
                format!(" {:<3}", index + 1)
            } else if show_numbers {
                format!("{:>3} ", index + 1)
            } else {
                String::new()
            };
            let continuation = " ".repeat(prefix.chars().count());
            let width = (inner.width as usize)
                .saturating_sub(prefix.chars().count())
                .max(1);
            let wrapped = textwrap::wrap(verse, width);

            verse_starts.push(lines.len());
            for (part_index, part) in wrapped.iter().enumerate() {
                let line_prefix = if part_index == 0 {
                    prefix.as_str()
                } else {
                    continuation.as_str()
                };
                if rtl {
                    let shown = bidi::visual(part);
                    let pad = " ".repeat(width.saturating_sub(bidi::width(&shown)));
                    lines.push(Line::from(vec![
                        Span::raw(pad),
                        Span::styled(shown.into_owned(), verse_style),
                        Span::styled(line_prefix.to_string(), verse_style),
                    ]));
                } else {
                    lines.push(Line::from(vec![
                        Span::styled(line_prefix.to_string(), verse_style),
                        Span::styled(part.to_string(), verse_style),
                    ]));
                }
            }

            lines.push(Line::from(""));
        }
    } else {
        lines.push(Line::from(Span::styled(
            "No chapter selected",
            Style::new().fg(color(theme.muted)),
        )));
    }

    let height = inner.height.max(1) as usize;
    let max_scroll = lines.len().saturating_sub(height).min(u16::MAX as usize) as u16;

    if app.focus == Focus::Verses
        && let Some(&start) = verse_starts.get(app.verse_index)
    {
        let scroll = app.scroll as usize;
        if start < scroll {
            app.scroll = start.min(u16::MAX as usize) as u16;
        } else if start >= scroll.saturating_add(height) {
            app.scroll = start
                .saturating_sub(height.saturating_sub(1))
                .min(u16::MAX as usize) as u16;
        }
    }

    app.scroll = app.scroll.min(max_scroll);
    app.visible_verses = visible_verse_count(&verse_starts, app.scroll as usize, height);

    let paragraph = Paragraph::new(lines).block(block).scroll((app.scroll, 0));
    frame.render_widget(paragraph, area);
}

fn visible_verse_count(starts: &[usize], scroll: usize, height: usize) -> usize {
    if starts.is_empty() || height == 0 {
        return 0;
    }

    let end = scroll.saturating_add(height);
    starts
        .iter()
        .filter(|&&start| start >= scroll && start < end)
        .count()
        .max(1)
}

fn render_footer(frame: &mut Frame, area: Rect, app: &App) {
    let hints = match app.focus {
        Focus::Books => {
            "Books: j/k · h/l switch pane · Enter: chapters · [/] chapter · m mark · / search · S study · D add · t translations · T theme · , settings · ? help · q quit"
        }
        Focus::Chapters => {
            "Chapters: j/k · h/l switch pane · Enter: verses · [/] chapter · m mark · / search · S study · D add · t translations · T theme · , settings · ? help · q quit"
        }
        Focus::Verses => {
            "Verses: j/k · h/l switch pane · Ctrl+d/u page · gg/G top/bottom · m mark · M note · / search · n/N next/prev · [/] chapter · S study · D add · q quit"
        }
    };
    let text = match &app.message {
        Some(message) => format!(" {message}  ·  {hints}"),
        None => format!(" {hints}"),
    };

    frame.render_widget(
        Paragraph::new(text).style(Style::new().fg(color(app.theme().muted))),
        area,
    );
}

fn verse_title(app: &App) -> String {
    match (app.book(), app.chapter()) {
        (Some(book), Some(chapter)) => {
            bidi::visual(&format!(" {} {} ", book.name, chapter.number)).into_owned()
        }
        _ => " Verses ".to_string(),
    }
}

fn panel_block(
    theme: &oma_bible_core::theme::Theme,
    title: impl Into<Line<'static>>,
    active: bool,
) -> Block<'static> {
    let border_color = if active { theme.accent } else { theme.muted };

    Block::bordered()
        .title(title)
        .border_style(Style::new().fg(color(border_color)))
        .style(Style::new().bg(color(theme.background)))
}

fn selection_style(theme: &oma_bible_core::theme::Theme, active: bool) -> Style {
    if active {
        Style::new()
            .fg(color(theme.bright_foreground))
            .bg(color(theme.selection))
            .add_modifier(Modifier::BOLD)
    } else {
        Style::new().fg(color(theme.foreground))
    }
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);

    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    }
}

fn color(value: Rgb) -> Color {
    Color::Rgb(value.red, value.green, value.blue)
}

fn render_screen(frame: &mut Frame, area: Rect, app: &mut App) {
    let [header_area, body_area, footer_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(area);

    render_screen_header(frame, header_area, app);

    match app.screen {
        Screen::Themes => render_themes(frame, body_area, app),
        Screen::Translations => render_translations(frame, body_area, app),
        Screen::Download => render_download(frame, body_area, app),
        Screen::Languages => render_languages(frame, body_area, app),
        Screen::Study => render_study(frame, body_area, app),
        Screen::Settings => render_settings(frame, body_area, app),
        Screen::Help => render_help(frame, body_area, app),
        Screen::Reader => {}
    }

    render_screen_footer(frame, footer_area, app);
}

fn render_screen_header(frame: &mut Frame, area: Rect, app: &App) {
    let theme = app.theme();
    let mut spans = vec![
        Span::styled(
            " Oma Bible ",
            Style::new()
                .fg(color(theme.background))
                .bg(color(theme.accent))
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            screen_title(app.screen),
            Style::new()
                .fg(color(theme.foreground))
                .add_modifier(Modifier::BOLD),
        ),
    ];

    if let Some(translation) = app.translation() {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            format!("{} ({})", translation.name, translation.abbreviation),
            Style::new().fg(color(theme.muted)),
        ));
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn screen_title(screen: Screen) -> &'static str {
    match screen {
        Screen::Reader => "Reader",
        Screen::Themes => "Appearance",
        Screen::Translations => "Translations",
        Screen::Download => "Add a translation",
        Screen::Languages => "Languages",
        Screen::Study => "Bookmarks & study",
        Screen::Settings => "Settings",
        Screen::Help => "Keyboard shortcuts",
    }
}

fn render_themes(frame: &mut Frame, area: Rect, app: &App) {
    let theme = app.theme();
    let current = app.themes.current_index();

    let items = app
        .themes
        .themes()
        .iter()
        .enumerate()
        .map(|(index, candidate)| {
            let active = current == Some(index);
            let marker = if active { "● " } else { "  " };
            let mut line = vec![
                Span::styled(marker, Style::new().fg(color(theme.accent))),
                Span::styled(
                    candidate.name.clone(),
                    Style::new()
                        .fg(color(theme.foreground))
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("  {}", candidate.mode.label()),
                    Style::new().fg(color(theme.muted)),
                ),
            ];

            if candidate.slug != candidate.name {
                line.push(Span::styled(
                    format!("  ·  {}", candidate.slug),
                    Style::new().fg(color(theme.muted)),
                ));
            }

            if active {
                line.push(Span::styled(
                    "  active",
                    Style::new().fg(color(theme.green)),
                ));
            }

            ListItem::new(Line::from(line))
        })
        .collect::<Vec<_>>();

    let subtitle = if app.themes.is_on_omarchy() {
        format!(
            "Following the Omarchy system theme · {} theme{} discovered",
            app.themes.themes().len(),
            if app.themes.themes().len() == 1 {
                ""
            } else {
                "s"
            }
        )
    } else {
        String::from("Not running under Omarchy · the selected theme is remembered")
    };

    let [note_area, list_area] =
        Layout::vertical([Constraint::Length(2), Constraint::Min(1)]).areas(area);
    frame.render_widget(
        Paragraph::new(subtitle).style(Style::new().fg(color(theme.muted))),
        note_area,
    );

    render_list(
        frame,
        list_area,
        theme,
        " Themes ",
        items,
        Some(app.theme_selected),
        "No Omarchy themes found. Oma Dark is always available.",
    );
}

fn render_translations(frame: &mut Frame, area: Rect, app: &App) {
    let theme = app.theme();
    let translations = app.filtered_translations();

    let items = translations
        .iter()
        .map(|translation| {
            let chapters = translation
                .books
                .iter()
                .map(|book| book.chapter_count())
                .sum::<usize>();
            ListItem::new(Line::from(vec![
                Span::styled(
                    translation.name.clone(),
                    Style::new()
                        .fg(color(theme.foreground))
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("  ({})", translation.abbreviation),
                    Style::new().fg(color(theme.accent)),
                ),
                Span::styled(
                    format!(
                        "  ·  {}  ·  {} books, {} chapters",
                        translation.language,
                        translation.books.len(),
                        chapters
                    ),
                    Style::new().fg(color(theme.muted)),
                ),
            ]))
        })
        .collect::<Vec<_>>();

    let (note_area, list_area) = filter_note(area, &app.translation_query);
    if let Some(note_area) = note_area {
        frame.render_widget(
            Paragraph::new(app.translation_query.clone())
                .style(Style::new().fg(color(theme.accent))),
            note_area,
        );
    }

    render_list(
        frame,
        list_area,
        theme,
        " Translations ",
        items,
        Some(app.translation_selected),
        "No translations installed. Press D to add one from the Bible API.",
    );
}

fn render_download(frame: &mut Frame, area: Rect, app: &App) {
    let theme = app.theme();
    let mut note_lines = Vec::new();

    if let Some(download) = &app.download {
        note_lines.push(Line::from(vec![
            Span::styled(
                format!("Downloading {} ({}) ", download.name, download.id),
                Style::new()
                    .fg(color(theme.foreground))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(
                    "· {} downloaded · Esc cancels",
                    format_bytes(download.control.downloaded())
                ),
                Style::new().fg(color(theme.accent)),
            ),
        ]));
    } else if app.catalog_loading {
        note_lines.push(Line::from(Span::styled(
            "Contacting bible.helloao.org for the translation catalogue…",
            Style::new().fg(color(theme.muted)),
        )));
    } else if let Some(error) = &app.catalog_error {
        note_lines.push(Line::from(Span::styled(
            format!("Catalogue unavailable: {error}"),
            Style::new().fg(color(theme.red)),
        )));
        note_lines.push(Line::from(Span::styled(
            "Press r to try again.",
            Style::new().fg(color(theme.muted)),
        )));
    } else if let Some(catalog) = &app.catalog {
        note_lines.push(Line::from(vec![
            Span::styled(
                format!(
                    "{} translation{}",
                    catalog.translations().len(),
                    if catalog.translations().len() == 1 {
                        ""
                    } else {
                        "s"
                    }
                ),
                Style::new().fg(color(theme.foreground)),
            ),
            Span::styled(
                format!("  ·  Language: {}", language_filter_label(app)),
                Style::new().fg(color(theme.accent)),
            ),
            Span::styled(
                "  ·  l filter · r refresh",
                Style::new().fg(color(theme.muted)),
            ),
        ]));
    } else {
        note_lines.push(Line::from(Span::styled(
            "No catalogue loaded.",
            Style::new().fg(color(theme.muted)),
        )));
    }

    if !app.download_query.is_empty() || !app.catalog_loading {
        let query = app.download_query.trim();
        if !query.is_empty() && app.download.is_none() {
            note_lines.push(Line::from(Span::styled(
                format!("Search: {query}"),
                Style::new().fg(color(theme.muted)),
            )));
        }
    }

    let note_height = note_lines.len().max(1) as u16;
    let [note_area, list_area] =
        Layout::vertical([Constraint::Length(note_height), Constraint::Min(1)]).areas(area);
    frame.render_widget(Paragraph::new(note_lines), note_area);

    let items = app
        .filtered_remote()
        .iter()
        .map(|translation| {
            let mut spans = vec![
                Span::styled(
                    translation.title().to_string(),
                    Style::new()
                        .fg(color(theme.foreground))
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("  ({})", translation.id),
                    Style::new().fg(color(theme.accent)),
                ),
            ];

            if !translation.english_name.trim().is_empty()
                && translation.english_name.trim() != translation.title()
            {
                spans.push(Span::styled(
                    format!("  ·  {}", translation.english_name.trim()),
                    Style::new().fg(color(theme.muted)),
                ));
            }

            spans.push(Span::styled(
                format!(
                    "  ·  {}  ·  {} books  ·  {} verses",
                    translation.language_label(),
                    translation.number_of_books,
                    translation.total_number_of_verses
                ),
                Style::new().fg(color(theme.muted)),
            ));

            ListItem::new(Line::from(spans))
        })
        .collect::<Vec<_>>();

    render_list(
        frame,
        list_area,
        theme,
        " Available from Bible API ",
        items,
        Some(app.download_selected),
        if app.catalog_loading {
            "Loading…"
        } else {
            "No translations match. Press / to search or l to filter by language."
        },
    );
}

fn render_languages(frame: &mut Frame, area: Rect, app: &App) {
    let theme = app.theme();
    let options = app.language_options();

    let items = options
        .iter()
        .map(|option| {
            let active = option.code == app.download_language;
            let marker = if active { "● " } else { "  " };
            ListItem::new(Line::from(vec![
                Span::styled(marker, Style::new().fg(color(theme.accent))),
                Span::styled(
                    option.label.clone(),
                    Style::new()
                        .fg(color(theme.foreground))
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(
                        "  ·  {} translation{}",
                        option.count,
                        if option.count == 1 { "" } else { "s" }
                    ),
                    Style::new().fg(color(theme.muted)),
                ),
            ]))
        })
        .collect::<Vec<_>>();

    let (note_area, list_area) = filter_note(area, &app.language_query);
    if let Some(note_area) = note_area {
        frame.render_widget(
            Paragraph::new(app.language_query.clone()).style(Style::new().fg(color(theme.accent))),
            note_area,
        );
    }

    render_list(
        frame,
        list_area,
        theme,
        " Filter by language ",
        items,
        Some(app.language_selected),
        "The catalogue is still loading.",
    );
}

fn render_study(frame: &mut Frame, area: Rect, app: &App) {
    let theme = app.theme();
    let items = app
        .bookmarks
        .items()
        .iter()
        .rev()
        .map(|bookmark| {
            let mut line = vec![
                Span::styled(
                    bookmark.reference(),
                    Style::new()
                        .fg(color(theme.foreground))
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("  {}", bookmark.translation),
                    Style::new().fg(color(theme.accent)),
                ),
            ];

            if !bookmark.note.is_empty() {
                line.push(Span::styled(
                    format!("  ·  {}", bookmark.note),
                    Style::new().fg(color(theme.muted)),
                ));
            }

            ListItem::new(Line::from(line))
        })
        .collect::<Vec<_>>();

    render_list(
        frame,
        area,
        theme,
        format!(" Bookmarks ({}) ", app.bookmarks.len()),
        items,
        Some(app.study_selected),
        "No bookmarks yet. Press m on a verse in the reader to mark it.",
    );
}

fn render_settings(frame: &mut Frame, area: Rect, app: &App) {
    let theme = app.theme();
    let translation = app
        .translation()
        .map(|translation| format!("{} ({})", translation.name, translation.abbreviation))
        .unwrap_or_else(|| String::from("None"));
    let downloads = user_data_dir().display().to_string();

    let rows: [(String, String); 7] = [
        (String::from("Theme"), app.themes.active().name.clone()),
        (
            String::from("Verse numbers"),
            String::from(if app.settings.show_verse_numbers {
                "On"
            } else {
                "Off"
            }),
        ),
        (String::from("Translation"), translation),
        (
            String::from("Add a translation"),
            String::from("Download from bible.helloao.org"),
        ),
        (
            String::from("Bookmarks"),
            format!("{} saved", app.bookmarks.len()),
        ),
        (String::from("Downloads folder"), downloads),
        (
            String::from("Keyboard shortcuts"),
            String::from("Open help"),
        ),
    ];

    let items = rows
        .iter()
        .map(|(label, value)| {
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{label:<22}"),
                    Style::new().fg(color(theme.foreground)),
                ),
                Span::styled(value.clone(), Style::new().fg(color(theme.muted))),
            ]))
        })
        .collect::<Vec<_>>();

    render_list(
        frame,
        area,
        theme,
        " Settings ",
        items,
        Some(app.settings_selected),
        "",
    );
}

fn render_help(frame: &mut Frame, area: Rect, app: &App) {
    let theme = app.theme();
    let keys = [
        ("Reader", "Navigate"),
        ("j / k", "Move down / up in the focused pane"),
        ("h / l, Tab", "Move between books, chapters and verses"),
        ("gg / G", "Jump to the first or last item"),
        ("Ctrl+d / Ctrl+u", "Move half a page"),
        ("[ / ]", "Previous / next chapter"),
        ("Enter", "Move focus forward"),
        ("/", "Search this chapter"),
        ("n / N", "Next / previous search match"),
        ("m", "Toggle a bookmark on the selected verse"),
        ("M", "Add or edit a note for the selected verse"),
        ("S", "Bookmarks & study screen"),
        ("t", "Switch translations"),
        ("D", "Add a translation from the Bible API"),
        ("T", "Choose a theme"),
        (",", "Settings"),
        ("?", "This help screen"),
        ("q / Esc", "Leave the current screen or quit"),
    ];

    let mut lines = Vec::new();
    for (key, description) in keys {
        if description == "Navigate" {
            lines.push(Line::from(Span::styled(
                key,
                Style::new()
                    .fg(color(theme.accent))
                    .add_modifier(Modifier::BOLD),
            )));
        } else {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {key:<16}"),
                    Style::new().fg(color(theme.foreground)),
                ),
                Span::styled(description, Style::new().fg(color(theme.muted))),
            ]));
        }
    }

    lines.push(Line::from(""));
    for (key, description) in [
        ("Study", "Bookmarks"),
        ("j / k, Enter", "Select and jump to a bookmark"),
        ("x", "Remove the selected bookmark"),
        ("M", "Edit the selected bookmark's note"),
        ("API", "Add a translation"),
        ("j / k, Enter", "Select and download a translation"),
        ("/ / l / r", "Search · language filter · refresh"),
        ("Esc", "Cancel a download or leave the screen"),
    ] {
        if description == "Bookmarks" || description == "Add a translation" {
            lines.push(Line::from(Span::styled(
                key,
                Style::new()
                    .fg(color(theme.accent))
                    .add_modifier(Modifier::BOLD),
            )));
        } else {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {key:<16}"),
                    Style::new().fg(color(theme.foreground)),
                ),
                Span::styled(description, Style::new().fg(color(theme.muted))),
            ]));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "These shortcuts also work in the GTK app.",
        Style::new().fg(color(theme.muted)),
    )));

    let block = panel_block(theme, " Keys ", true);
    frame.render_widget(Paragraph::new(Text::from(lines)).block(block), area);
}

fn render_list(
    frame: &mut Frame,
    area: Rect,
    theme: &oma_bible_core::theme::Theme,
    title: impl Into<Line<'static>>,
    items: Vec<ListItem<'static>>,
    selected: Option<usize>,
    placeholder: &str,
) {
    let block = panel_block(theme, title, true);

    if items.is_empty() {
        frame.render_widget(
            Paragraph::new(placeholder.to_string())
                .block(block)
                .style(Style::new().fg(color(theme.muted))),
            area,
        );
        return;
    }

    let list = List::new(items)
        .block(block)
        .highlight_style(selection_style(theme, true))
        .highlight_symbol("› ");
    let mut state = ListState::default().with_selected(selected);
    frame.render_stateful_widget(list, area, &mut state);
}

fn render_screen_footer(frame: &mut Frame, area: Rect, app: &App) {
    let hints = match app.screen {
        Screen::Themes => "j/k move · Enter select · Esc back",
        Screen::Translations => "j/k move · Enter read · / filter · D add · Esc back",
        Screen::Download => {
            if app.download.is_some() {
                "Esc cancel"
            } else {
                "j/k move · Enter download · / search · l filter language · r refresh · Esc back"
            }
        }
        Screen::Languages => "j/k move · Enter select · / search · Esc back",
        Screen::Study => "j/k move · Enter jump · x remove · M edit note · Esc back",
        Screen::Settings => "j/k move · Enter open/change · Esc back",
        Screen::Help => "Esc back",
        Screen::Reader => "",
    };
    let text = match &app.message {
        Some(message) => format!(" {message}  ·  {hints}"),
        None => format!(" {hints}"),
    };

    frame.render_widget(
        Paragraph::new(text).style(Style::new().fg(color(app.theme().muted))),
        area,
    );
}

fn render_input(frame: &mut Frame, area: Rect, app: &App, prompt: &crate::app::InputPrompt) {
    let theme = app.theme();
    let overlay = centered_rect(64.min(area.width), 3, area);
    frame.render_widget(Clear, overlay);

    let block = Block::bordered()
        .title(format!(" {} ", prompt.label))
        .border_style(Style::new().fg(color(theme.accent)))
        .style(Style::new().bg(color(theme.background)));

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                prompt.value.clone(),
                Style::new().fg(color(theme.bright_foreground)),
            ),
            Span::styled("▏", Style::new().fg(color(theme.accent))),
        ]))
        .block(block),
        overlay,
    );
}

fn filter_note(area: Rect, query: &str) -> (Option<Rect>, Rect) {
    if query.trim().is_empty() {
        return (None, area);
    }

    let [note_area, list_area] =
        Layout::vertical([Constraint::Length(2), Constraint::Min(1)]).areas(area);
    (Some(note_area), list_area)
}

fn language_filter_label(app: &App) -> String {
    match &app.download_language {
        None => String::from("all"),
        Some(code) => app
            .catalog
            .as_ref()
            .and_then(|catalog| {
                catalog
                    .languages()
                    .into_iter()
                    .find(|language| &language.code == code)
                    .map(|language| language.name)
            })
            .unwrap_or_else(|| code.clone()),
    }
}

fn format_bytes(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = KIB * 1024;

    if bytes >= MIB {
        format!("{:.1} MiB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.1} KiB", bytes as f64 / KIB as f64)
    } else {
        format!("{bytes} B")
    }
}
