//! The study sheet for one verse: cross-references, a commentary, the
//! translation's footnotes and the original Hebrew or Greek words, each on
//! its own tab. Data comes from the Free
//! Use Bible API on demand and is cached on disk by the core crate.

use std::collections::HashMap;

use gpui_kit::assets::IconName;
use gpui_kit::component::{ActiveTheme, Icon, h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;
use oma_bible_core::bible::canon;
use oma_bible_core::original::{self, ClickedWord, InterlinearChapter, LexiconEntry, OriginalWord};
use oma_bible_core::study::{
    self, ChapterReferences, Commentary, CommentaryChapter, CrossReference,
};

use crate::app::{OmaBibleApp, Overlay};

/// Commentary used until the reader picks another.
pub const DEFAULT_COMMENTARY: &str = "matthew-henry";
/// Cross-references shown per verse, strongest first.
const MAX_REFERENCES: usize = 40;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExploreTab {
    References,
    Commentary,
    Footnotes,
    Original,
}

impl ExploreTab {
    const ALL: [Self; 4] = [
        Self::References,
        Self::Commentary,
        Self::Footnotes,
        Self::Original,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::References => "Cross-references",
            Self::Commentary => "Commentary",
            Self::Footnotes => "Footnotes",
            Self::Original => "Original",
        }
    }

    fn step(self, delta: isize) -> Self {
        let index = Self::ALL.iter().position(|tab| *tab == self).unwrap_or(0) as isize;
        let len = Self::ALL.len() as isize;
        Self::ALL[(index + delta).rem_euclid(len) as usize]
    }
}

/// A request that may still be on its way.
#[derive(Clone, Debug)]
pub enum Fetch<T> {
    Loading,
    Ready(T),
    Failed(String),
}

/// The verse the sheet is about.
#[derive(Clone, Debug)]
pub struct ExploreTarget {
    pub book_index: usize,
    /// USFM code; `None` when the translation does not say which book it is.
    pub book_id: Option<String>,
    pub book_name: String,
    pub chapter: u32,
    pub verse: u32,
}

/// Study sheet state kept on the app.
pub struct Explore {
    pub tab: ExploreTab,
    pub selected: usize,
    pub target: Option<ExploreTarget>,
    pub references: HashMap<(String, u32), Fetch<ChapterReferences>>,
    pub commentaries: HashMap<(String, String, u32), Fetch<Option<CommentaryChapter>>>,
    pub commentary_list: Option<Fetch<Vec<Commentary>>>,
    pub interlinear: HashMap<(String, u32), Fetch<InterlinearChapter>>,
    pub lexicon: HashMap<String, Fetch<Option<LexiconEntry>>>,
    /// The original word picked on the Original tab.
    pub word: Option<usize>,
    /// The word double-clicked in the reader, matched to an original word
    /// once the chapter's words arrive.
    pub clicked: Option<ClickedWord>,
    pub scroll: ScrollHandle,
}

impl Default for Explore {
    fn default() -> Self {
        Self {
            tab: ExploreTab::References,
            selected: 0,
            target: None,
            references: HashMap::new(),
            commentaries: HashMap::new(),
            commentary_list: None,
            interlinear: HashMap::new(),
            lexicon: HashMap::new(),
            word: None,
            clicked: None,
            scroll: ScrollHandle::new(),
        }
    }
}

impl OmaBibleApp {
    pub fn commentary_id(&self) -> String {
        self.settings
            .commentary
            .clone()
            .unwrap_or_else(|| DEFAULT_COMMENTARY.to_string())
    }

    /// Open the study sheet on the selected verse at `tab`.
    pub fn open_explore(&mut self, tab: ExploreTab, cx: &mut Context<Self>) {
        let (Some(translation), Some(book), Some(chapter)) =
            (self.translation(), self.book(), self.chapter())
        else {
            self.set_status("No verse selected", cx);
            return;
        };
        let target = ExploreTarget {
            book_index: self.book_index,
            book_id: translation.book_id(self.book_index),
            book_name: book.name.clone(),
            chapter: chapter.number,
            verse: self.verse_index as u32 + 1,
        };
        self.explore.tab = tab;
        self.explore.selected = 0;
        self.explore.word = None;
        self.explore.clicked = None;
        self.explore.target = Some(target);
        self.explore.scroll.set_offset(point(px(0.), px(0.)));
        self.overlay = Overlay::Explore;
        self.load_explore(cx);
        cx.notify();
    }

    /// Open the Original tab on the selected verse, picking the original
    /// word behind `clicked` when it can be matched.
    pub fn open_word(&mut self, clicked: Option<ClickedWord>, cx: &mut Context<Self>) {
        self.open_explore(ExploreTab::Original, cx);
        self.explore.clicked = clicked;
        self.ensure_word_lexicon(cx);
    }

    /// Fetch the list of commentaries once.
    pub fn ensure_commentary_list(&mut self, cx: &mut Context<Self>) {
        if self.explore.commentary_list.is_none() {
            self.explore.commentary_list = Some(Fetch::Loading);
            cx.spawn(async move |this, cx| {
                let result = cx
                    .background_executor()
                    .spawn(async move { study::commentaries().map_err(|e| e.to_string()) })
                    .await;
                this.update(cx, |app, cx| {
                    app.explore.commentary_list = Some(into_fetch(result));
                    cx.notify();
                })
                .ok();
            })
            .detach();
        }
    }

    /// Commentaries for the settings dropdown, if loaded.
    pub fn commentary_options(&self) -> Option<&[Commentary]> {
        match &self.explore.commentary_list {
            Some(Fetch::Ready(list)) => Some(list),
            _ => None,
        }
    }

    /// Use a commentary from now on.
    pub fn choose_commentary(&mut self, id: &str, cx: &mut Context<Self>) {
        let title = self
            .commentary_options()
            .and_then(|list| list.iter().find(|c| c.id == id))
            .map(|c| c.title().to_string())
            .unwrap_or_else(|| id.to_string());
        self.settings.commentary = Some(id.to_string());
        let _ = self.settings.save();
        self.status = format!("Commentary · {title}").into();
        if self.overlay == Overlay::Explore {
            self.load_explore(cx);
        }
        cx.notify();
    }

    fn load_explore(&mut self, cx: &mut Context<Self>) {
        let Some(target) = self.explore.target.clone() else {
            return;
        };
        self.ensure_commentary_list(cx);
        let Some(book) = target.book_id else {
            return;
        };
        let chapter = target.chapter;

        let key = (book.clone(), chapter);
        if !self.explore.references.contains_key(&key) {
            self.explore.references.insert(key.clone(), Fetch::Loading);
            let book = book.clone();
            cx.spawn(async move |this, cx| {
                let result = cx
                    .background_executor()
                    .spawn(async move {
                        study::cross_references(&book, chapter).map_err(|e| e.to_string())
                    })
                    .await;
                this.update(cx, |app, cx| {
                    app.explore.references.insert(key, into_fetch(result));
                    cx.notify();
                })
                .ok();
            })
            .detach();
        }

        let key = (book.clone(), chapter);
        if !self.explore.interlinear.contains_key(&key) {
            self.explore.interlinear.insert(key.clone(), Fetch::Loading);
            let book = book.clone();
            cx.spawn(async move |this, cx| {
                let result = cx
                    .background_executor()
                    .spawn(async move {
                        original::interlinear(&book, chapter).map_err(|e| e.to_string())
                    })
                    .await;
                this.update(cx, |app, cx| {
                    app.explore.interlinear.insert(key, into_fetch(result));
                    app.ensure_word_lexicon(cx);
                    cx.notify();
                })
                .ok();
            })
            .detach();
        }

        let commentary = self.commentary_id();
        let key = (commentary.clone(), book.clone(), chapter);
        if !self.explore.commentaries.contains_key(&key) {
            self.explore
                .commentaries
                .insert(key.clone(), Fetch::Loading);
            cx.spawn(async move |this, cx| {
                let result = cx
                    .background_executor()
                    .spawn(async move {
                        study::commentary_chapter(&commentary, &book, chapter)
                            .map_err(|e| e.to_string())
                    })
                    .await;
                this.update(cx, |app, cx| {
                    app.explore.commentaries.insert(key, into_fetch(result));
                    cx.notify();
                })
                .ok();
            })
            .detach();
        }
    }

    fn set_explore_tab(&mut self, tab: ExploreTab, cx: &mut Context<Self>) {
        self.explore.tab = tab;
        self.explore.selected = 0;
        self.explore.scroll.set_offset(point(px(0.), px(0.)));
        if tab == ExploreTab::Original {
            self.ensure_word_lexicon(cx);
        }
        cx.notify();
    }

    /// The original words of the target verse.
    fn target_words(&self) -> Option<Fetch<&[OriginalWord]>> {
        let target = self.explore.target.as_ref()?;
        let book = target.book_id.clone()?;
        Some(
            match self.explore.interlinear.get(&(book, target.chapter))? {
                Fetch::Loading => Fetch::Loading,
                Fetch::Failed(error) => Fetch::Failed(error.clone()),
                Fetch::Ready(chapter) => Fetch::Ready(
                    chapter
                        .get(&target.verse)
                        .map(Vec::as_slice)
                        .unwrap_or_default(),
                ),
            },
        )
    }

    /// The picked original word: chosen on the tab, else matched from the
    /// word double-clicked in the reader, else the first.
    fn picked_word(&self) -> Option<usize> {
        let Some(Fetch::Ready(words)) = self.target_words() else {
            return None;
        };
        if words.is_empty() {
            return None;
        }
        self.explore
            .word
            .or_else(|| {
                self.explore
                    .clicked
                    .as_ref()
                    .and_then(|clicked| original::match_word(clicked, words))
            })
            .map(|index| index.min(words.len() - 1))
            .or(Some(0))
    }

    fn pick_word(&mut self, index: usize, cx: &mut Context<Self>) {
        self.explore.word = Some(index);
        self.ensure_word_lexicon(cx);
        cx.notify();
    }

    /// Load the lexicon entry of the picked word.
    fn ensure_word_lexicon(&mut self, cx: &mut Context<Self>) {
        let Some(Fetch::Ready(words)) = self.target_words() else {
            return;
        };
        let Some(strongs) = self
            .picked_word()
            .and_then(|index| words.get(index))
            .and_then(|word| word.strongs.clone())
        else {
            return;
        };
        if self.explore.lexicon.contains_key(&strongs) {
            return;
        }
        self.explore.lexicon.insert(strongs.clone(), Fetch::Loading);
        cx.spawn(async move |this, cx| {
            let id = strongs.clone();
            let result = cx
                .background_executor()
                .spawn(async move { original::lexicon(&id).map_err(|e| e.to_string()) })
                .await;
            this.update(cx, |app, cx| {
                app.explore.lexicon.insert(strongs, into_fetch(result));
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn step_word(&mut self, delta: isize, cx: &mut Context<Self>) {
        let count = match self.target_words() {
            Some(Fetch::Ready(words)) => words.len(),
            _ => 0,
        };
        if count == 0 {
            return;
        }
        let current = self.picked_word().unwrap_or(0) as isize;
        self.pick_word((current + delta).rem_euclid(count as isize) as usize, cx);
    }

    /// Switch to the next commentary the API offers.
    pub fn cycle_commentary(&mut self, cx: &mut Context<Self>) {
        let Some(Fetch::Ready(list)) = &self.explore.commentary_list else {
            return;
        };
        if list.is_empty() {
            return;
        }
        let current = self.commentary_id();
        let index = list
            .iter()
            .position(|commentary| commentary.id == current)
            .map(|index| (index + 1) % list.len())
            .unwrap_or(0);
        let next = list[index].clone();
        self.settings.commentary = Some(next.id.clone());
        let _ = self.settings.save();
        self.status = format!("Commentary · {}", next.title()).into();
        self.explore.tab = ExploreTab::Commentary;
        self.load_explore(cx);
        cx.notify();
    }

    /// Cross-references of the target verse, strongest first.
    fn target_references(&self) -> Option<Fetch<Vec<CrossReference>>> {
        let target = self.explore.target.as_ref()?;
        let book = target.book_id.clone()?;
        Some(
            match self.explore.references.get(&(book, target.chapter))? {
                Fetch::Loading => Fetch::Loading,
                Fetch::Failed(error) => Fetch::Failed(error.clone()),
                Fetch::Ready(chapter) => Fetch::Ready(
                    chapter
                        .get(&target.verse)
                        .map(|refs| refs.iter().take(MAX_REFERENCES).cloned().collect())
                        .unwrap_or_default(),
                ),
            },
        )
    }

    fn reference_count(&self) -> usize {
        match self.target_references() {
            Some(Fetch::Ready(refs)) => refs.len(),
            _ => 0,
        }
    }

    fn open_selected_reference(&mut self, cx: &mut Context<Self>) {
        let Some(Fetch::Ready(refs)) = self.target_references() else {
            return;
        };
        if let Some(reference) = refs.get(self.explore.selected).cloned() {
            self.jump_to_reference(&reference, cx);
        }
    }

    /// Go to a cross-reference in the open translation.
    pub fn jump_to_reference(&mut self, reference: &CrossReference, cx: &mut Context<Self>) {
        let label = reference_label(self, reference);
        let Some(book_index) = self
            .translation()
            .and_then(|translation| translation.book_index_by_id(&reference.book))
        else {
            self.set_status(format!("{label} is not in this translation"), cx);
            return;
        };
        if self.jump_to_place(book_index, reference.chapter, reference.verse) {
            self.overlay = Overlay::None;
            self.set_status(format!("Jumped to {label}"), cx);
        } else {
            self.set_status(format!("{label} is not in this translation"), cx);
        }
    }

    pub fn handle_explore_key(&mut self, key: &str, shift: bool, cx: &mut Context<Self>) {
        match key {
            "escape" | "q" => self.close_overlay(cx),
            "tab" if shift => self.set_explore_tab(self.explore.tab.step(-1), cx),
            "tab" | "l" | "right" => self.set_explore_tab(self.explore.tab.step(1), cx),
            "h" | "left" => self.set_explore_tab(self.explore.tab.step(-1), cx),
            "1" => self.set_explore_tab(ExploreTab::References, cx),
            "2" => self.set_explore_tab(ExploreTab::Commentary, cx),
            "3" => self.set_explore_tab(ExploreTab::Footnotes, cx),
            // Vim word motions walk the original words.
            "w" if self.explore.tab == ExploreTab::Original => self.step_word(1, cx),
            "b" if self.explore.tab == ExploreTab::Original => self.step_word(-1, cx),
            "4" | "w" => self.set_explore_tab(ExploreTab::Original, cx),
            "c" => self.cycle_commentary(cx),
            "j" | "down" | "k" | "up" if self.explore.tab == ExploreTab::References => {
                let count = self.reference_count();
                if count > 0 {
                    let down = matches!(key, "j" | "down");
                    self.explore.selected = if down {
                        (self.explore.selected + 1) % count
                    } else {
                        (self.explore.selected + count - 1) % count
                    };
                    self.explore.scroll.scroll_to_item(self.explore.selected);
                    cx.notify();
                }
            }
            "j" | "down" | "k" | "up" => {
                let offset = self.explore.scroll.offset();
                let delta = if matches!(key, "j" | "down") {
                    -60.
                } else {
                    60.
                };
                let max = self.explore.scroll.max_offset();
                let y = (offset.y + px(delta)).clamp(-max.y, px(0.));
                self.explore.scroll.set_offset(point(offset.x, y));
                cx.notify();
            }
            "enter" if self.explore.tab == ExploreTab::References => {
                self.open_selected_reference(cx)
            }
            _ => {}
        }
    }
}

fn into_fetch<T>(result: Result<T, String>) -> Fetch<T> {
    match result {
        Ok(value) => Fetch::Ready(value),
        Err(error) => Fetch::Failed(error),
    }
}

/// `Romans 5:8` or `1 John 4:9-10`, using the open translation's book name
/// when it has the book.
fn reference_label(app: &OmaBibleApp, reference: &CrossReference) -> String {
    let name = app
        .translation()
        .and_then(|translation| {
            translation
                .book_index_by_id(&reference.book)
                .map(|index| translation.books[index].name.clone())
        })
        .unwrap_or_else(|| canon::english_name(&reference.book).to_string());
    match reference.end_verse.filter(|end| *end > reference.verse) {
        Some(end) => format!("{name} {}:{}-{end}", reference.chapter, reference.verse),
        None => format!("{name} {}:{}", reference.chapter, reference.verse),
    }
}

/// The referenced verses' text in the open translation, if it has them.
fn reference_text(app: &OmaBibleApp, reference: &CrossReference) -> Option<String> {
    let translation = app.translation()?;
    let book = &translation.books[translation.book_index_by_id(&reference.book)?];
    let chapter = book.chapter(reference.chapter)?;
    let end = reference
        .end_verse
        .unwrap_or(reference.verse)
        .max(reference.verse);
    let text = (reference.verse..=end.min(reference.verse + 5))
        .filter_map(|verse| chapter.verse(verse))
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    (!text.is_empty()).then_some(text)
}

pub fn render_explore(app: &OmaBibleApp, cx: &mut Context<OmaBibleApp>) -> AnyElement {
    let Some(target) = app.explore.target.clone() else {
        return crate::app::overlay_shell(cx, 640., div());
    };
    let reference = format!("{} {}:{}", target.book_name, target.chapter, target.verse);
    let footnotes: Vec<String> = app
        .translation()
        .and_then(|translation| translation.books.get(target.book_index))
        .and_then(|book| book.chapter(target.chapter))
        .map(|chapter| {
            chapter
                .footnotes_for(target.verse)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();

    let tabs = h_flex().gap(px(6.)).px(px(18.)).pb(px(12.)).children(
        ExploreTab::ALL.iter().enumerate().map(|(index, &tab)| {
            let active = tab == app.explore.tab;
            let count = match tab {
                ExploreTab::References => app.reference_count(),
                ExploreTab::Footnotes => footnotes.len(),
                ExploreTab::Commentary => 0,
                ExploreTab::Original => match app.target_words() {
                    Some(Fetch::Ready(words)) => words.len(),
                    _ => 0,
                },
            };
            let hover = cx.theme().muted;
            h_flex()
                .id(SharedString::from(format!("explore-tab-{index}")))
                .items_center()
                .gap(px(6.))
                .px(px(12.))
                .py(px(5.))
                .rounded_full()
                .cursor_pointer()
                .text_size(px(12.))
                .when(active, |pill| {
                    pill.bg(cx.theme().accent)
                        .text_color(cx.theme().accent_foreground)
                })
                .when(!active, |pill| {
                    pill.text_color(cx.theme().muted_foreground)
                        .hover(move |style| style.bg(hover))
                })
                .on_click(cx.listener(move |this, _, _, cx| this.set_explore_tab(tab, cx)))
                .child(tab.label())
                .when(count > 0, |pill| {
                    pill.child(div().text_size(px(10.)).child(count.to_string()))
                })
        }),
    );

    let body = match app.explore.tab {
        ExploreTab::References => references_body(app, &target, cx),
        ExploreTab::Commentary => commentary_body(app, &target, cx),
        ExploreTab::Footnotes => footnotes_body(&footnotes, cx),
        ExploreTab::Original => original_body(app, &target, cx),
    };

    let hint = match app.explore.tab {
        ExploreTab::References => "Tab switch · j/k move · Enter go to verse · Esc close",
        ExploreTab::Commentary => "Tab switch · j/k scroll · c next commentary · Esc close",
        ExploreTab::Footnotes => "Tab switch · Esc close",
        ExploreTab::Original => {
            "Tab switch · w/b next/previous word · j/k scroll · double-click a word in the text to look it up · Esc close"
        }
    };

    let card = v_flex()
        .w_full()
        .rounded(px(18.))
        .bg(cx.theme().popover)
        .border_1()
        .border_color(cx.theme().border)
        .shadow_xl()
        .overflow_hidden()
        .child(
            v_flex()
                .px(px(24.))
                .pt(px(20.))
                .pb(px(14.))
                .gap(px(3.))
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(cx.theme().muted_foreground)
                        .child("STUDY"),
                )
                .child(div().text_size(px(18.)).child(reference)),
        )
        .child(tabs)
        .child(
            div()
                .id("explore-body")
                .w_full()
                .h(px(440.))
                .overflow_y_scroll()
                .track_scroll(&app.explore.scroll)
                .px(px(18.))
                .pb(px(16.))
                .child(body),
        )
        .child(
            h_flex()
                .w_full()
                .px(px(24.))
                .py(px(10.))
                .border_t_1()
                .border_color(cx.theme().border)
                .text_size(px(11.))
                .text_color(cx.theme().muted_foreground)
                .child(hint),
        );

    crate::app::overlay_shell(cx, 680., card)
}

fn message(text: impl Into<SharedString>, cx: &App) -> AnyElement {
    div()
        .py(px(18.))
        .px(px(6.))
        .text_size(px(14.))
        .line_height(px(22.))
        .text_color(cx.theme().muted_foreground)
        .child(text.into())
        .into_any_element()
}

fn references_body(
    app: &OmaBibleApp,
    target: &ExploreTarget,
    cx: &mut Context<OmaBibleApp>,
) -> AnyElement {
    if target.book_id.is_none() {
        return message(
            "This translation does not say which book this is. Download it again to see cross-references.",
            cx,
        );
    }
    let refs = match app.target_references() {
        None | Some(Fetch::Loading) => return message("Loading cross-references…", cx),
        Some(Fetch::Failed(error)) => {
            return message(format!("Could not load cross-references: {error}"), cx);
        }
        Some(Fetch::Ready(refs)) => refs,
    };
    if refs.is_empty() {
        return message("No cross-references for this verse.", cx);
    }
    let accent = cx.theme().accent;
    let border = cx.theme().border;
    v_flex()
        .w_full()
        .gap(px(8.))
        .children(refs.into_iter().enumerate().map(|(index, reference)| {
            let label = reference_label(app, &reference);
            let text = reference_text(app, &reference);
            let picked = index == app.explore.selected;
            v_flex()
                .id(SharedString::from(format!("explore-ref-{index}")))
                .w_full()
                .gap(px(4.))
                .px(px(14.))
                .py(px(10.))
                .rounded(px(12.))
                .bg(cx.theme().background)
                .border_1()
                .border_color(if picked { accent.opacity(0.7) } else { border })
                .cursor_pointer()
                .hover(move |style| style.border_color(accent.opacity(0.8)))
                .on_click(cx.listener(move |this, _, _, cx| this.jump_to_reference(&reference, cx)))
                .child(
                    h_flex()
                        .gap(px(6.))
                        .items_center()
                        .text_size(px(12.))
                        .text_color(accent)
                        .child(Icon::new(IconName::Link).size(px(12.)))
                        .child(label),
                )
                .child(
                    div()
                        .text_size(px(14.))
                        .line_height(px(22.))
                        .text_color(if text.is_some() {
                            cx.theme().foreground
                        } else {
                            cx.theme().muted_foreground
                        })
                        .child(crate::rtl::paragraph(
                            text.unwrap_or_else(|| "Not in this translation".to_string()),
                            Some(3),
                        )),
                )
        }))
        .into_any_element()
}

fn commentary_body(
    app: &OmaBibleApp,
    target: &ExploreTarget,
    cx: &mut Context<OmaBibleApp>,
) -> AnyElement {
    let Some(book) = target.book_id.clone() else {
        return message(
            "This translation does not say which book this is. Download it again to see commentaries.",
            cx,
        );
    };
    let id = app.commentary_id();
    let name = match &app.explore.commentary_list {
        Some(Fetch::Ready(list)) => list
            .iter()
            .find(|commentary| commentary.id == id)
            .map(|commentary| commentary.title().to_string())
            .unwrap_or_else(|| id.clone()),
        _ => id.clone(),
    };
    let chooser = h_flex()
        .id("explore-commentary")
        .items_center()
        .gap(px(6.))
        .mb(px(12.))
        .px(px(12.))
        .py(px(6.))
        .rounded_full()
        .cursor_pointer()
        .bg(cx.theme().muted)
        .text_size(px(12.))
        .text_color(cx.theme().foreground)
        .hover({
            let hover = cx.theme().selection;
            move |style| style.bg(hover)
        })
        .on_click(cx.listener(|this, _, _, cx| this.cycle_commentary(cx)))
        .child(Icon::new(IconName::BookText).size(px(13.)))
        .child(name)
        .child(
            div()
                .text_color(cx.theme().muted_foreground)
                .child("· c to change"),
        );

    let content = match app.explore.commentaries.get(&(id, book, target.chapter)) {
        None | Some(Fetch::Loading) => message("Loading commentary…", cx),
        Some(Fetch::Failed(error)) => message(format!("Could not load commentary: {error}"), cx),
        Some(Fetch::Ready(None)) => message(
            "This commentary does not cover this book. Press c to try another.",
            cx,
        ),
        Some(Fetch::Ready(Some(chapter))) => match chapter.for_verse(target.verse) {
            Some(section) => {
                let covers = match section.end {
                    Some(end) => format!("On verses {}–{end}", section.start),
                    None if section.start == target.verse => format!("On verse {}", section.start),
                    None => format!("From verse {}", section.start),
                };
                v_flex()
                    .w_full()
                    .gap(px(10.))
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(cx.theme().muted_foreground)
                            .child(covers.to_uppercase()),
                    )
                    .children(paragraphs(section.text, cx))
                    .into_any_element()
            }
            None if !chapter.introduction.is_empty() => v_flex()
                .w_full()
                .gap(px(10.))
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(cx.theme().muted_foreground)
                        .child("CHAPTER INTRODUCTION"),
                )
                .children(paragraphs(&chapter.introduction, cx))
                .into_any_element(),
            None => message("No commentary on this verse.", cx),
        },
    };

    v_flex()
        .w_full()
        .child(div().flex().child(chooser))
        .child(content)
        .into_any_element()
}

fn paragraphs(text: &str, cx: &App) -> Vec<AnyElement> {
    text.split('\n')
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| {
            div()
                .w_full()
                .text_size(px(15.))
                .line_height(px(25.))
                .text_color(cx.theme().foreground)
                .child(line.to_string())
                .into_any_element()
        })
        .collect()
}

fn footnotes_body(footnotes: &[String], cx: &App) -> AnyElement {
    if footnotes.is_empty() {
        return message(
            "No footnotes on this verse in this translation. Translations downloaded before this update need downloading again to show footnotes.",
            cx,
        );
    }
    v_flex()
        .w_full()
        .gap(px(8.))
        .children(footnotes.iter().map(|note| {
            div()
                .w_full()
                .px(px(14.))
                .py(px(10.))
                .rounded(px(12.))
                .bg(cx.theme().background)
                .border_1()
                .border_color(cx.theme().border)
                .text_size(px(14.))
                .line_height(px(22.))
                .child(crate::rtl::paragraph(note.clone(), None))
        }))
        .into_any_element()
}

fn original_body(
    app: &OmaBibleApp,
    target: &ExploreTarget,
    cx: &mut Context<OmaBibleApp>,
) -> AnyElement {
    if target.book_id.is_none() {
        return message(
            "This translation does not say which book this is. Download it again to see the original words.",
            cx,
        );
    }
    let words = match app.target_words() {
        None | Some(Fetch::Loading) => return message("Loading the original words…", cx),
        Some(Fetch::Failed(error)) => {
            return message(format!("Could not load the original words: {error}"), cx);
        }
        Some(Fetch::Ready(words)) => words,
    };
    if words.is_empty() {
        return message("No original-language words for this verse.", cx);
    }
    let picked = app.picked_word().unwrap_or(0);
    let rtl = words.first().is_some_and(OriginalWord::is_rtl);
    let muted = cx.theme().muted_foreground;
    let accent = cx.theme().accent;
    let border = cx.theme().border;

    // What happened to the word double-clicked in the text.
    let note = match &app.explore.clicked {
        Some(clicked) if app.explore.word.is_none() => match original::match_word(clicked, words) {
            Some(_) => format!(
                "“{}” in the text",
                clicked.text.trim_matches(|c: char| !c.is_alphanumeric())
            ),
            None => format!(
                "“{}” cannot be linked to a single original word in this translation. Pick one below.",
                clicked.text
            ),
        },
        _ => "Pick a word to see what it means.".to_string(),
    };

    let chips = words.iter().enumerate().map(|(index, word)| {
        let is_picked = index == picked;
        let hover = cx.theme().muted;
        v_flex()
            .id(SharedString::from(format!("original-word-{index}")))
            .items_center()
            .gap(px(1.))
            .px(px(10.))
            .py(px(6.))
            .rounded(px(10.))
            .border_1()
            .cursor_pointer()
            .border_color(if is_picked { accent } else { border })
            .when(is_picked, |chip| chip.bg(cx.theme().muted))
            .when(!is_picked, |chip| chip.hover(move |style| style.bg(hover)))
            .on_click(cx.listener(move |this, _, _, cx| this.pick_word(index, cx)))
            .child(
                div()
                    .text_size(px(19.))
                    .text_color(cx.theme().foreground)
                    .child(word.text.clone()),
            )
            .child(
                div()
                    .text_size(px(10.))
                    .text_color(muted)
                    .child(word.transliteration.clone().unwrap_or_default()),
            )
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(if is_picked { accent } else { muted })
                    .child(word.gloss.clone().unwrap_or_else(|| "—".to_string())),
            )
    });

    let flow = h_flex()
        .w_full()
        .flex_wrap()
        .gap(px(6.))
        .when(rtl, |flow| flow.flex_row_reverse())
        .children(chips);

    v_flex()
        .w_full()
        .gap(px(14.))
        .child(div().text_size(px(12.)).text_color(muted).child(note))
        .child(flow)
        .when_some(words.get(picked), |body, word| {
            body.child(word_detail(app, word, cx))
        })
        .child(
            div()
                .pt(px(6.))
                .text_size(px(10.))
                .line_height(px(15.))
                .text_color(muted)
                .child(
                    "Words: Berean Standard Bible interlinear (public domain). Lexicon: STEPBible.org / Tyndale House brief lexicons (TBESH, TBESG), CC BY 4.0.",
                ),
        )
        .into_any_element()
}

/// The picked word: its form, parsing, and the lexicon entry.
fn word_detail(app: &OmaBibleApp, word: &OriginalWord, cx: &App) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let foreground = cx.theme().foreground;
    let language = match word.language.as_str() {
        "hebrew" => "Hebrew",
        "aramaic" => "Aramaic",
        "greek" => "Greek",
        other => other,
    };
    let facts = [
        Some(language.to_string()),
        word.strongs
            .clone()
            .map(|strongs| format!("Strong's {strongs}")),
        word.parsing.clone(),
    ]
    .into_iter()
    .flatten()
    .filter(|fact| !fact.is_empty())
    .collect::<Vec<_>>()
    .join(" · ");

    let lexicon = match word
        .strongs
        .as_ref()
        .and_then(|strongs| app.explore.lexicon.get(strongs))
    {
        _ if word.strongs.is_none() => message("No Strong's number for this word.", cx),
        None | Some(Fetch::Loading) => message("Loading the lexicon…", cx),
        Some(Fetch::Failed(error)) => message(format!("Could not load the lexicon: {error}"), cx),
        Some(Fetch::Ready(None)) => message("No lexicon entry for this word.", cx),
        Some(Fetch::Ready(Some(entry))) => v_flex()
            .w_full()
            .gap(px(14.))
            .children(entry.senses.iter().map(|sense| {
                v_flex()
                    .w_full()
                    .gap(px(6.))
                    .child(
                        h_flex()
                            .items_baseline()
                            .gap(px(8.))
                            .child(
                                div()
                                    .text_size(px(20.))
                                    .text_color(foreground)
                                    .child(sense.lemma.clone()),
                            )
                            .child(div().text_size(px(12.)).text_color(muted).child(format!(
                                "{} · {}",
                                sense.transliteration, sense.part_of_speech
                            ))),
                    )
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(cx.theme().accent)
                            .child(sense.gloss.clone()),
                    )
                    .children(sense.paragraphs().into_iter().map(|line| {
                        div()
                            .w_full()
                            .text_size(px(14.))
                            .line_height(px(22.))
                            .text_color(foreground)
                            .child(line)
                    }))
            }))
            .into_any_element(),
    };

    v_flex()
        .w_full()
        .gap(px(10.))
        .p(px(16.))
        .rounded(px(12.))
        .bg(cx.theme().background)
        .border_1()
        .border_color(cx.theme().border)
        .child(
            h_flex()
                .items_baseline()
                .gap(px(10.))
                .child(
                    div()
                        .text_size(px(30.))
                        .text_color(foreground)
                        .child(word.text.clone()),
                )
                .child(
                    div()
                        .text_size(px(14.))
                        .text_color(muted)
                        .child(word.transliteration.clone().unwrap_or_default()),
                ),
        )
        .child(div().text_size(px(12.)).text_color(muted).child(facts))
        .when_some(word.parsing_full.clone(), |detail, parsing| {
            detail.child(
                div()
                    .text_size(px(13.))
                    .text_color(foreground)
                    .child(parsing),
            )
        })
        .when_some(word.gloss.clone(), |detail, gloss| {
            detail.child(
                div()
                    .text_size(px(13.))
                    .text_color(muted)
                    .child(format!("In this verse: “{gloss}”")),
            )
        })
        .child(div().h(px(1.)).w_full().bg(cx.theme().border))
        .child(lexicon)
        .into_any_element()
}
