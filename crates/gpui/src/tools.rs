//! The reader's quick actions, shared by the floating dock and the
//! right-click wheel. Adding a tool here makes it available to both; each
//! surface picks the subset it shows.

use gpui_kit::assets::IconName;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Bookmark,
    Note,
    Highlight,
    Copy,
    Study,
    Settings,
}

impl Tool {
    /// The dock stays minimal: verse actions, then settings after a divider.
    pub const DOCK: [Self; 5] = [
        Self::Bookmark,
        Self::Note,
        Self::Highlight,
        Self::Copy,
        Self::Settings,
    ];

    /// The wheel has room for every tool, clockwise from the top.
    pub const WHEEL: [Self; 6] = [
        Self::Bookmark,
        Self::Note,
        Self::Highlight,
        Self::Copy,
        Self::Study,
        Self::Settings,
    ];

    pub fn icon(self) -> IconName {
        match self {
            Self::Bookmark => IconName::Bookmark,
            Self::Note => IconName::StickyNote,
            Self::Highlight => IconName::Highlighter,
            Self::Copy => IconName::Copy,
            Self::Study => IconName::BookMarked,
            Self::Settings => IconName::Settings,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Bookmark => "Bookmark",
            Self::Note => "Note",
            Self::Highlight => "Highlight",
            Self::Copy => "Copy",
            Self::Study => "Study",
            Self::Settings => "Settings",
        }
    }

    /// The reader shortcut that does the same thing, shown as a hint.
    pub fn key(self) -> &'static str {
        match self {
            Self::Bookmark => "m",
            Self::Note => "M",
            Self::Highlight => "H",
            Self::Copy => "y",
            Self::Study => "S",
            Self::Settings => ",",
        }
    }

    /// Settings is app chrome, not a verse action; the dock separates it.
    pub fn is_verse_action(self) -> bool {
        !matches!(self, Self::Study | Self::Settings)
    }
}
