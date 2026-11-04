# Oma Bible

A native Bible reader for Omarchy Linux, written in Rust. The interface follows
the quiet, document-first look of [Omawrite](https://github.com/omacom/omawrite):
theme colours, restrained chrome, a centred reading column, and a muted footer.

## Workspace

| Crate | Purpose |
|---|---|
| `crates/core` | Bible data model, JSON loading, HelloAO downloads, themes, persistence (`oma-bible-core`) |
| `crates/tui` | Terminal reader (`oma-bible-tui`) |
| `crates/gtk` | GTK4 + libadwaita app (`oma-bible-gtk`) |

The core crate has no UI dependencies, so both front ends share the same models,
loaders, download client, settings, bookmarks, and theme catalog.

## Run

TUI:

```bash
cargo run -p oma-bible-tui
```

GTK:

```bash
cargo run -p oma-bible-gtk
```

The GTK app has a books/chapters sidebar, a centred reading column, a bookmarks
and study panel, and a slim header and footer. Downloads and settings open in
separate windows.

## Keys

The same Neovim-style keys work in both front ends.

| Key | Action |
|---|---|
| `j` / `k`, `Down` / `Up` | Move in the focused pane |
| `h` / `l`, `Left` / `Right`, `Tab` | Move between books, chapters, and verses |
| `gg` / `G`, `Home` / `End` | First or last item |
| `Ctrl+d` / `Ctrl+u`, `PgDn` / `PgUp` | Half page down or up |
| `[` / `]` | Previous or next chapter |
| `Enter` | Move focus forward |
| `/` | Search the current chapter |
| `n` / `N` | Next or previous search match |
| `m` | Toggle a bookmark on the selected verse |
| `M` | Add or edit a note for the selected verse |
| `S` | Bookmarks and study panel |
| `t` | Focus the translation chooser |
| `D` | Add a translation from the Bible API |
| `T` or `,` | Settings |
| `?` | Keyboard shortcut help |
| `q` / `Esc` | Leave the current screen, or quit |

Inside the study panel: `j` / `k` select a bookmark, `Enter` jumps to it, `x`
or `Delete` removes it, and `M` edits its note.

Inside the Bible API browser: `j` / `k` select a translation, `Enter`
downloads it, `/` searches, `l` changes the language filter, `r` refreshes the
catalogue, and `Esc` cancels an in-progress download or closes the window.

### Fonts for downloaded languages

When a translation is downloaded, Oma Bible checks whether the system has a
font for its script. Ethiopic (Amharic, Tigrinya, Ge'ez) is set up
automatically: if no Ethiopic font is installed, the app downloads Noto Sans
Ethiopic Condensed into `~/.local/share/fonts/oma-bible/` and refreshes the font cache.
It also writes
`~/.config/fontconfig/conf.d/60-oma-bible-ethiopic.conf`, which asks
fontconfig to prefer the narrower condensed face so Ethiopic letters fit the
fixed cell grid of a terminal. Restart the terminal once after the first
Amharic download to pick up the new font. Other scripts use the fonts already
installed by the system; when one is missing the app names the script so the
right package can be installed.

## Themes

On Omarchy, both front ends use the current Omarchy theme. Run
`omarchy theme set <name>` to change it; a running reader notices the change
within a second and redraws with the new colours. No restart is needed.

Off Omarchy, the apps still read the installed `colors.toml` files:

- **TUI:** opens the theme picker at startup when more than one theme is found.
- **GTK:** opens Settings on first run and keeps a theme dropdown there.

Themes are discovered from:

1. `~/.config/omarchy/themes/*/colors.toml` — user themes
2. `$OMARCHY_PATH/themes/*/colors.toml` — stock themes
3. `/usr/share/omarchy/themes/*/colors.toml` — stock fallback

User themes win when a slug exists in both places. `Oma Dark` is always
available as a neutral fallback.

## Bible API downloads

The **Add a translation** screen reads the catalogue from
[bible.helloao.org](https://bible.helloao.org/) and lists every available
translation, grouped and filterable by language. Downloads are streamed to disk
and can be cancelled at any time.

- Catalogue: `GET /api/available_translations.json`
- Translation: `GET /api/{id}/complete.simple.json`
- Downloads are saved as JSON in the user data directory and merged over
  bundled translations by abbreviation.
- The last language filter is remembered in settings.

Downloaded translations survive restarts. A download that shares an
abbreviation with a bundled file replaces it in the library.

## Data

Bundled translations live in `crates/core/data` as JSON files. The file shape
matches the Rust structs:

```json
{
  "name": "King James Version",
  "language": "English",
  "abbreviation": "KJV",
  "books": [
    {
      "name": "Genesis",
      "chapters": [
        { "number": 1, "verses": ["In the beginning..."] }
      ]
    }
  ]
}
```

Every `*.json` file in the data directory is loaded, then translations are
sorted by name. The bundled files are small samples; use **Add a translation**
for a complete Bible.

### Storage locations

| What | Default path | Override |
|---|---|---|
| Bundled data | `crates/core/data` | `OMA_BIBLE_DATA_DIR` |
| Downloaded translations | `$XDG_DATA_HOME/oma-bible` | `OMA_BIBLE_DOWNLOAD_DIR` |
| Settings | `$XDG_CONFIG_HOME/oma-bible/settings.json` | `OMA_BIBLE_CONFIG_DIR` |
| Bookmarks and notes | `$XDG_STATE_HOME/oma-bible/bookmarks.json` | `OMA_BIBLE_STATE_DIR` |

`OMA_BIBLE_DATA_DIR` also redirects downloads so a single override can point the
whole app at a test library.

## Checks

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Why GTK instead of Qt Quick?

Qt Quick is excellent at the kind of animated, custom-drawn interface Omawrite
uses, and Qt 6 is installed on this machine. The reason to stay on GTK for this
app is simplicity: `gtk-rs` is pure Rust, so there is one language, one build
system, and no C++ bridge. That matters while learning Rust.

Qt Quick would become the better choice if the app grows features that play to
QML's strengths: rich text editing, smooth page transitions, custom animations,
and pixel-level control over typography. The Rust options are `cxx-qt` (QML plus
a Rust backend through generated C++ bindings) or `qmetaobject` (QML with a more
macro-driven Rust integration). Both add Qt, QML, and a C++ toolchain to the
build.

For now the GTK front end reproduces the Omawrite feel with CSS classes and a
plain reading column, without that extra machinery.

## Version note

The GTK crates are pinned to `gtk4 0.10` and `libadwaita 0.8` because this
machine has Rust 1.90. The newer `gtk4 0.11` / `libadwaita 0.9` releases
require Rust 1.92. Upgrade both together when the toolchain is newer.
