//! Omarchy theme discovery and colors.
//!
//! Omarchy keeps the active theme in `$XDG_STATE_HOME/omarchy/current/theme`
//! and stores available themes in `$XDG_CONFIG_HOME/omarchy/themes` (user)
//! and `$OMARCHY_PATH/themes` (stock). Each theme is mostly a `colors.toml`.

use std::{
    collections::BTreeMap,
    env, fmt, fs, io,
    path::{Path, PathBuf},
};

use serde::Deserialize;

const FALLBACK_SLUG: &str = "oma-dark";
const STOCK_OMARCHY_PATH: &str = "/usr/share/omarchy";

/// A colour from an Omarchy `colors.toml` file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

impl Rgb {
    pub const fn new(red: u8, green: u8, blue: u8) -> Self {
        Self { red, green, blue }
    }

    /// Parse `#rrggbb` or `rrggbb`.
    pub fn parse(value: &str) -> Option<Self> {
        let hex = value.trim().strip_prefix('#').unwrap_or(value.trim());

        if hex.len() != 6 || !hex.is_ascii() {
            return None;
        }

        Some(Self {
            red: u8::from_str_radix(&hex[0..2], 16).ok()?,
            green: u8::from_str_radix(&hex[2..4], 16).ok()?,
            blue: u8::from_str_radix(&hex[4..6], 16).ok()?,
        })
    }

    pub fn to_hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.red, self.green, self.blue)
    }
}

impl fmt::Display for Rgb {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.to_hex())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeMode {
    Light,
    Dark,
}

impl ThemeMode {
    fn parse(value: &str) -> Self {
        if value.eq_ignore_ascii_case("light") {
            Self::Light
        } else {
            Self::Dark
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Light => "Light",
            Self::Dark => "Dark",
        }
    }

    pub fn is_dark(self) -> bool {
        self == Self::Dark
    }
}

/// The colours an Omarchy theme exposes to an application.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Theme {
    pub slug: String,
    pub name: String,
    pub mode: ThemeMode,
    pub accent: Rgb,
    pub selection: Rgb,
    pub muted: Rgb,
    pub background: Rgb,
    pub dark_background: Rgb,
    pub lighter_background: Rgb,
    pub foreground: Rgb,
    pub dark_foreground: Rgb,
    pub bright_foreground: Rgb,
    pub red: Rgb,
    pub yellow: Rgb,
    pub green: Rgb,
    pub blue: Rgb,
    pub magenta: Rgb,
    pub cyan: Rgb,
}

impl Theme {
    /// A calm, self-contained palette used when no Omarchy theme is available.
    pub fn fallback() -> Self {
        Self {
            slug: FALLBACK_SLUG.to_string(),
            name: "Oma Dark".to_string(),
            mode: ThemeMode::Dark,
            accent: Rgb::new(0x89, 0xb4, 0xfa),
            selection: Rgb::new(0x45, 0x47, 0x5a),
            muted: Rgb::new(0x6c, 0x70, 0x86),
            background: Rgb::new(0x1e, 0x1e, 0x2e),
            dark_background: Rgb::new(0x18, 0x18, 0x25),
            lighter_background: Rgb::new(0x31, 0x32, 0x44),
            foreground: Rgb::new(0xcd, 0xd6, 0xf4),
            dark_foreground: Rgb::new(0x7f, 0x84, 0x9c),
            bright_foreground: Rgb::new(0xf5, 0xf5, 0xff),
            red: Rgb::new(0xf3, 0x8b, 0xa8),
            yellow: Rgb::new(0xf9, 0xe2, 0xaf),
            green: Rgb::new(0xa6, 0xe3, 0xa1),
            blue: Rgb::new(0x89, 0xb4, 0xfa),
            magenta: Rgb::new(0xcb, 0xa6, 0xf7),
            cyan: Rgb::new(0x89, 0xdc, 0xeb),
        }
    }

    pub fn from_toml(
        slug: impl Into<String>,
        name: impl Into<String>,
        contents: &str,
    ) -> Result<Self, ThemeError> {
        let raw: RawTheme = toml::from_str(contents)?;
        let fallback = Self::fallback();

        let background = color(&raw.background, fallback.background);
        let dark_background = color(&raw.dark_background, background);
        let lighter_background = color(&raw.lighter_background, background);
        let foreground = color(&raw.foreground, fallback.foreground);
        let muted = color(&raw.muted, fallback.muted);
        let dark_foreground = color(&raw.dark_foreground, muted);
        let selection = color(&raw.selection, lighter_background);

        Ok(Self {
            slug: slug.into(),
            name: name.into(),
            mode: raw
                .mode
                .as_deref()
                .map(ThemeMode::parse)
                .unwrap_or(fallback.mode),
            accent: color(&raw.accent, fallback.accent),
            selection,
            muted,
            background,
            dark_background,
            lighter_background,
            foreground,
            dark_foreground,
            bright_foreground: color(&raw.bright_foreground, foreground),
            red: color(&raw.red, fallback.red),
            yellow: color(&raw.yellow, fallback.yellow),
            green: color(&raw.green, fallback.green),
            blue: color(&raw.blue, fallback.blue),
            magenta: color(&raw.magenta, fallback.magenta),
            cyan: color(&raw.cyan, fallback.cyan),
        })
    }

    fn from_file(path: &Path, slug: String, name: String) -> Result<Self, ThemeError> {
        let contents = fs::read_to_string(path)?;
        Self::from_toml(slug, name, &contents)
    }
}

#[derive(Deserialize)]
struct RawTheme {
    mode: Option<String>,
    accent: Option<String>,
    selection: Option<String>,
    muted: Option<String>,
    background: Option<String>,
    dark_background: Option<String>,
    lighter_background: Option<String>,
    foreground: Option<String>,
    dark_foreground: Option<String>,
    bright_foreground: Option<String>,
    red: Option<String>,
    yellow: Option<String>,
    green: Option<String>,
    blue: Option<String>,
    magenta: Option<String>,
    cyan: Option<String>,
}

fn color(value: &Option<String>, fallback: Rgb) -> Rgb {
    value.as_deref().and_then(Rgb::parse).unwrap_or(fallback)
}

#[derive(Debug)]
pub enum ThemeError {
    Io(io::Error),
    Parse(toml::de::Error),
}

impl fmt::Display for ThemeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "could not read theme: {error}"),
            Self::Parse(error) => write!(formatter, "could not parse theme: {error}"),
        }
    }
}

impl std::error::Error for ThemeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Parse(error) => Some(error),
        }
    }
}

impl From<io::Error> for ThemeError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<toml::de::Error> for ThemeError {
    fn from(error: toml::de::Error) -> Self {
        Self::Parse(error)
    }
}

/// Every known theme plus the one the app should currently use.
#[derive(Clone, Debug)]
pub struct ThemeCatalog {
    themes: Vec<Theme>,
    active: Theme,
    on_omarchy: bool,
}

impl ThemeCatalog {
    /// Discover themes and pick the active one.
    ///
    /// On Omarchy the active theme is the one Omarchy is currently using. Off
    /// Omarchy the first discovered theme is used, with `Oma Dark` always
    /// available as a neutral choice.
    pub fn load() -> Self {
        let on_omarchy = detect_omarchy();
        let mut themes = discover_themes();
        let mut active = None;

        if on_omarchy && let Some(theme) = load_active_theme() {
            active = Some(theme);
        }

        if let Some(theme) = &active {
            match themes.iter_mut().find(|item| item.slug == theme.slug) {
                Some(slot) => *slot = theme.clone(),
                None => {
                    themes.push(theme.clone());
                    sort_themes(&mut themes);
                }
            }
        }

        if !on_omarchy && !themes.iter().any(|theme| theme.slug == FALLBACK_SLUG) {
            themes.push(Theme::fallback());
            sort_themes(&mut themes);
        }

        let active = active
            .or_else(|| {
                if on_omarchy {
                    None
                } else {
                    themes.first().cloned()
                }
            })
            .unwrap_or_else(Theme::fallback);

        Self {
            themes,
            active,
            on_omarchy,
        }
    }

    /// A catalog containing only the built-in theme, useful for tests.
    pub fn fallback_only() -> Self {
        Self {
            themes: vec![Theme::fallback()],
            active: Theme::fallback(),
            on_omarchy: false,
        }
    }

    pub fn themes(&self) -> &[Theme] {
        &self.themes
    }

    pub fn active(&self) -> &Theme {
        &self.active
    }

    pub fn is_on_omarchy(&self) -> bool {
        self.on_omarchy
    }

    pub fn current_index(&self) -> Option<usize> {
        self.themes
            .iter()
            .position(|theme| theme.slug == self.active.slug)
    }

    pub fn select(&mut self, index: usize) -> bool {
        match self.themes.get(index) {
            Some(theme) => {
                self.active = theme.clone();
                true
            }
            None => false,
        }
    }

    pub fn select_slug(&mut self, slug: &str) -> bool {
        match self
            .themes
            .iter()
            .position(|theme| theme.slug.eq_ignore_ascii_case(slug))
        {
            Some(index) => self.select(index),
            None => false,
        }
    }

    /// Re-read the active Omarchy theme.
    ///
    /// Omarchy swaps the `current/theme` link when the desktop theme changes.
    /// Polling this keeps a running reader in step with the rest of the
    /// session without restarting it. Off Omarchy the app keeps whatever theme
    /// the user picked, so this is a no-op.
    pub fn refresh_active(&mut self) -> bool {
        if !self.on_omarchy {
            return false;
        }

        let Some(theme) = load_active_theme() else {
            return false;
        };
        if theme == self.active {
            return false;
        }

        match self
            .themes
            .iter_mut()
            .find(|candidate| candidate.slug == theme.slug)
        {
            Some(slot) => *slot = theme.clone(),
            None => {
                self.themes.push(theme.clone());
                sort_themes(&mut self.themes);
            }
        }

        self.active = theme;
        true
    }
}

/// `true` when the app is running inside an Omarchy session.
pub fn detect_omarchy() -> bool {
    if env::var_os("OMARCHY_PATH").is_some() {
        return true;
    }

    [
        "DESKTOP_SESSION",
        "XDG_CURRENT_DESKTOP",
        "XDG_SESSION_DESKTOP",
    ]
    .iter()
    .filter_map(|key| env::var(key).ok())
    .any(|value| value.to_ascii_lowercase().contains("omarchy"))
}

/// Turn `ethio-dusk` into `Ethio Dusk`.
pub fn title_from_slug(slug: &str) -> String {
    slug.split(['-', '_'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn discover_themes() -> Vec<Theme> {
    let mut by_slug = BTreeMap::new();

    // User themes come first so they overlay stock themes with the same slug.
    for directory in theme_directories() {
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };

        for entry in entries.flatten() {
            let path = entry.path().join("colors.toml");
            if !path.is_file() {
                continue;
            }

            let slug = entry.file_name().to_string_lossy().into_owned();
            if by_slug.contains_key(&slug) {
                continue;
            }

            if let Ok(theme) = Theme::from_file(&path, slug.clone(), title_from_slug(&slug)) {
                by_slug.insert(slug, theme);
            }
        }
    }

    let mut themes = by_slug.into_values().collect::<Vec<_>>();
    sort_themes(&mut themes);
    themes
}

fn sort_themes(themes: &mut [Theme]) {
    themes.sort_by_cached_key(|theme| theme.name.to_lowercase());
}

fn theme_directories() -> Vec<PathBuf> {
    let mut directories = Vec::new();

    if let Some(config) = config_home() {
        directories.push(config.join("omarchy/themes"));
    }

    directories.push(omarchy_root().join("themes"));
    directories
}

fn config_home() -> Option<PathBuf> {
    env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
}

fn state_home() -> Option<PathBuf> {
    env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state")))
}

fn omarchy_root() -> PathBuf {
    env::var_os("OMARCHY_PATH")
        .map(PathBuf::from)
        .filter(|path| path.is_dir())
        .unwrap_or_else(|| PathBuf::from(STOCK_OMARCHY_PATH))
}

fn load_active_theme() -> Option<Theme> {
    let state = state_home()?;
    let path = state.join("omarchy/current/theme/colors.toml");

    if !path.is_file() {
        return None;
    }

    let slug = fs::read_to_string(state.join("omarchy/current/theme.name"))
        .ok()
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "current".to_string());

    Theme::from_file(&path, slug.clone(), title_from_slug(&slug)).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r##"
        mode = "dark"
        accent = "#d9a441"
        selection = "#4a3e30"
        muted = "#6b5c48"
        background = "#262019"
        foreground = "#e7d3b1"
        bright_foreground = "#f5e6c8"
    "##;

    #[test]
    fn parses_hex_colours_with_or_without_a_hash() {
        assert_eq!(Rgb::parse("#d9a441"), Some(Rgb::new(0xd9, 0xa4, 0x41)));
        assert_eq!(Rgb::parse("d9a441"), Some(Rgb::new(0xd9, 0xa4, 0x41)));
    }

    #[test]
    fn rejects_broken_hex_colours() {
        assert_eq!(Rgb::parse(""), None);
        assert_eq!(Rgb::parse("#fff"), None);
        assert_eq!(Rgb::parse("#gggggg"), None);
    }

    #[test]
    fn missing_colours_fall_back_to_the_built_in_palette() {
        let theme = Theme::from_toml("sample", "Sample", SAMPLE).unwrap();
        let fallback = Theme::fallback();

        assert_eq!(theme.background, Rgb::new(0x26, 0x20, 0x19));
        assert_eq!(theme.red, fallback.red);
        assert_eq!(theme.cyan, fallback.cyan);
        assert_eq!(theme.dark_background, theme.background);
    }

    #[test]
    fn slugs_become_readable_names() {
        assert_eq!(title_from_slug("ethio-dusk"), "Ethio Dusk");
        assert_eq!(title_from_slug("flexoki_light"), "Flexoki Light");
    }

    #[test]
    fn a_fallback_catalog_is_never_empty() {
        let catalog = ThemeCatalog::fallback_only();

        assert_eq!(catalog.themes().len(), 1);
        assert_eq!(catalog.active().slug, FALLBACK_SLUG);
        assert!(!catalog.is_on_omarchy());
    }
}
