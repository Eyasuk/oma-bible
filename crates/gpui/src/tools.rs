//! The reader's quick actions, shared by the floating dock and the
//! right-click wheel. Adding a tool here makes it available to both; each
//! surface picks the subset it shows.

use gpui_kit::assets::IconName;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Chapters,
    Bookmark,
    Note,
    Highlight,
    Copy,
    Explore,
    Study,
    Settings,
}

impl Tool {
    /// The dock stays minimal: verse actions, then navigation and settings
    /// after a divider.
    pub const DOCK: [Self; 7] = [
        Self::Bookmark,
        Self::Note,
        Self::Highlight,
        Self::Copy,
        Self::Explore,
        Self::Chapters,
        Self::Settings,
    ];

    /// The wheel holds the reading actions, clockwise from the top.
    /// Settings stays in the dock and on `,`.
    pub const WHEEL: [Self; 7] = [
        Self::Chapters,
        Self::Bookmark,
        Self::Note,
        Self::Highlight,
        Self::Copy,
        Self::Explore,
        Self::Study,
    ];

    pub fn icon(self) -> IconName {
        match self {
            Self::Chapters => IconName::BookOpen,
            Self::Bookmark => IconName::Bookmark,
            Self::Note => IconName::StickyNote,
            Self::Highlight => IconName::Highlighter,
            Self::Copy => IconName::Copy,
            Self::Explore => IconName::Waypoints,
            Self::Study => IconName::BookMarked,
            Self::Settings => IconName::Settings,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Chapters => "Chapters",
            Self::Bookmark => "Bookmark",
            Self::Note => "Note",
            Self::Highlight => "Highlight",
            Self::Copy => "Copy",
            Self::Explore => "Study verse",
            Self::Study => "Study",
            Self::Settings => "Settings",
        }
    }

    /// The reader shortcut that does the same thing, shown as a hint.
    pub fn key(self) -> &'static str {
        match self {
            Self::Chapters => "o",
            Self::Bookmark => "m",
            Self::Note => "M",
            Self::Highlight => "H",
            Self::Copy => "y",
            Self::Explore => "r",
            Self::Study => "S",
            Self::Settings => ",",
        }
    }

    /// Navigation and settings are app chrome, not verse actions; the dock
    /// separates them.
    pub fn is_verse_action(self) -> bool {
        !matches!(self, Self::Chapters | Self::Study | Self::Settings)
    }
}
