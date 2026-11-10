# Oma Bible

A native Bible reader for Omarchy Linux, written in Rust. The interface follows
the quiet, document-first look of [Omawrite](https://github.com/omacom/omawrite):
theme colours, restrained chrome, a centred reading column, and a muted footer.

## Workspace

| Crate | Purpose |
|---|---|
| `crates/core` | Bible data model, JSON loading, HelloAO downloads, themes, persistence (`oma-bible-core`) |
| `crates/tui` | Terminal reader (`oma-bible-tui`) |
| `crates/gpui` | GPU-accelerated desktop app (`oma-bible-gpui`, GPUI + gpui-kit/components) |

The core crate has no UI dependencies, so front ends share the same models,
loaders, download client, settings, bookmarks, and theme catalog.

## Run

TUI:

```bash
cargo run -p oma-bible-tui
```

GPUI (desktop):

```bash
cargo run -p oma-bible-gpui
```

The GPUI app is the primary desktop frontend: a single centred reading column,
small icon-only chrome, and popup overlays for book/chapter navigation,
settings, themes, downloads, and study.

## Keys

The TUI and GPUI apps share the same Neovim-style reading keys. The TUI also
uses `h` / `l` and `Tab` to move between its books, chapters, and verses panes;
in GPUI, `h` / `l` change chapters directly.

| Key | Action |
|---|---|
| `j` / `k`, `Down` / `Up` | Previous or next verse |
| `h` / `l`, `Left` / `Right` | Previous or next chapter |
| `gg` / `G`, `Home` / `End` | First or last verse |
| `Ctrl+d` / `Ctrl+u`, `PgDn` / `PgUp` | Page down or up |
| `o` / `Enter` | Open the book and chapter navigator (GPUI) |
| `/` | Search the current chapter |
| `n` / `N` | Next or previous search match |
| `y` | Copy the selected verse (GPUI) |
| `Ctrl+C` | Copy selected text; quit when nothing is selected (GPUI) |
| `m` | Toggle a bookmark on the selected verse |
| `M` | Open the selected verse's notes, or write a new one (GPUI: a verse can hold many notes) |
| `H` | Highlight the selected verse (GPUI) |
| `r` | Study the selected verse: cross-references, commentary, footnotes (GPUI) |
| `c` | Open the study sheet on the commentary tab (GPUI) |
| `Ctrl+Shift+D` | Move the shortcut dock to the next edge, or hide it (GPUI) |
| `Space` / right-click | Open the action wheel (GPUI) |
| `S` | Bookmarks and study screen |
| `t` | Installed translations |
| `T` | Themes |
| `D` | Add a translation from the Bible API |
| `,` | Settings overlay (GPUI) |
| `?` | Keyboard shortcut help |
| `q` / `Esc` | Leave the current screen, or quit |

Inside the navigator: `j` / `k` select a book, `h` / `l` select a chapter,
and `Enter` applies the selection.

In the GPUI reader, `y` copies the selected verse. To copy part of the text,
drag across it — a selection can span several verses — or double-click a word,
then press `Ctrl+C`. `Ctrl+C` with no selection quits.

To change chapter with the mouse, click the `BOOK 1 ⌄` title above the text to
open the book and chapter navigator, or use the `‹` / `›` arrows beside it for
the previous or next chapter. The Chapters button in the dock and wheel opens
the navigator too.

To act on several verses at once, drag across them, Shift+click to extend from
the selected verse, or Ctrl+click to pick verses one by one (they need not be
next to each other). Bookmark, highlight, and copy then apply to every selected
verse; `M` writes one note that starts with all selected verses quoted. `Esc`
clears the selection. Highlights are drawn as a marker stroke under each line of
words and stay visible while a verse is selected.

The small pill at the bottom is the shortcut dock: bookmark, note, highlight,
copy, chapters, and settings. `Ctrl+Shift+D` (or the Dock row in settings) moves it to the
left, top, or right edge, or hides it; the choice is saved.

Right-click anywhere in the text to open the action wheel around the pointer;
the verse under the pointer becomes the selected one. `Space` opens it in the
middle of the window. The hub names the tool under the pointer and its key. Press
`1`–`8` or a tool's own key (`o`, `m`, `M`, `H`, `y`, `r`, `S`, `,`) to pick one, or `Esc`
to close.

Notes in the open chapter lie in a loose pile at the middle of the right edge
(the left edge when the dock is on the right), the back cards scattered at
random angles. Hover it to spread every note out as its own card; click one to
jump to that verse and open its notes.

A verse can carry any number of notes (GPUI). The notes sheet shows each note as
a card: `j` / `k` pick one, `e` or `Enter` edits it, `x` deletes it, `n` writes
a new one. In the note editor, `Enter` starts a new line, `Ctrl+Enter` saves,
`Esc` cancels, and `Ctrl+I` quotes the note's verse (or the reader selection).
Type a reference such as `John 3:16-18` in the field under the text and press
`Enter` to quote any verse from the open translation. Quoted verses are lines
starting with `>`. Notes live in `notes.json`; the first time it is missing, the
notes already kept on bookmarks are copied in.

The study sheet (`r`, `c`, or **Study verse** on the wheel) has three tabs:
**Cross-references** from the open-cross-ref dataset, strongest first, shown in
the open translation (click one or press `Enter` to go there); **Commentary**
from one of the API's commentaries (Matthew Henry by default; `c` or the
commentary button switches, and **Settings → Commentary** opens a dropdown of
every commentary; the choice is saved), showing the section that
covers the verse; and **Footnotes** from the translation itself. `Tab`, `h` /
`l` or `1`–`3` switch tabs. Study data is fetched once per chapter and cached in
`$XDG_CACHE_HOME/oma-bible` for offline use. Section headings from the
translation appear above the verses they introduce.

Some translations have empty versification gaps (for example Revelation 1:5–6 in
the Amharic text). The GPUI reader hides empty verses and `j` / `k` skip them.
An empty verse that carries a bookmark or note stays visible, with a `—`
placeholder instead of text so the annotation remains reachable.

Inside the highlight sheet: `j` / `k` or `h` / `l` choose a colour, `Enter`
applies it, and `x` removes the highlight. The number keys `1`–`4` pick a colour
directly; `5` removes it.

Inside the study panel: `j` / `k` select a bookmark, `Enter` jumps to it, `x`
or `Delete` removes it, and `M` opens its notes.

Inside the Bible API browser: `j` / `k` select a translation, `Enter`
downloads it, `/` searches, `l` changes the language filter, `r` refreshes the
catalogue, and `Esc` cancels an in-progress download or closes the screen.

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

### Right-to-left languages

Arabic, Hebrew, Persian, Urdu, Syriac and other right-to-left translations are
detected from their text and read from the right: lines wrap in reading order
and line up against the right edge, and the verse number, cursor bar and
section headings move to the right side. The desktop app keeps the full
vowel marks and highlights.

Most terminals (Alacritty, Ghostty, foot, kitty) neither reorder right-to-left
text nor join Arabic letters, so the TUI does both itself. Terminal fonts
cannot draw vowel points on a letter, so the TUI shows Arabic and Hebrew
without harakat or niqqud. Set `OMA_BIBLE_TERMINAL_MARKS=1` to keep them, and
`OMA_BIBLE_TERMINAL_BIDI=1` in a terminal that does its own bidi (GNOME
Terminal/VTE, Konsole, mlterm) so the text is not reversed twice.

## Themes

On Omarchy, both front ends use the current Omarchy theme. Run
`omarchy theme set <name>` to change it; a running reader notices the change
within a second and redraws with the new colours. No restart is needed. In the
GPUI theme screen this list is read-only and marks the active Omarchy theme.

Off Omarchy, the apps still read the installed `colors.toml` files:

- **TUI:** opens the theme picker at startup when more than one theme is found.
- **GPUI:** starts with a palette matching the system light/dark appearance. A
  theme chosen in the theme screen is saved in settings and wins over the
  system appearance on later launches.

Themes are discovered from:

1. `~/.config/omarchy/themes/*/colors.toml` — user themes
2. `$OMARCHY_PATH/themes/*/colors.toml` — stock themes
3. `/usr/share/omarchy/themes/*/colors.toml` — stock fallback

User themes win when a slug exists in both places. `Oma Dark` and `Oma Light`
are always available as neutral fallbacks off Omarchy.

## Bible API downloads

The **Add a translation** screen reads the catalogue from
[bible.helloao.org](https://bible.helloao.org/) and lists every available
translation, grouped and filterable by language. Downloads are streamed to disk
and can be cancelled at any time. In the desktop app a toolbar above the list has a
language button (same as `l`), a search field (same as `/`, with a clear
button) and Refresh (same as `r`).

- Catalogue: `GET /api/available_translations.json`
- Translation: `GET /api/{id}/complete.simple.json`
- Downloads are saved as JSON in the user data directory. There are no
  built-in translations: a fresh install opens the download screen.
- The last language filter is remembered in settings.

Downloaded translations survive restarts. The reader shows exactly what the API
provides; some API translations are incomplete (for example `amh_amh` is the
New Testament only and is missing verses in the source data).

The catalogue also lists **መጽሐፍ ቅዱስ (ሙሉ)** (`amh_full`), the complete Amharic
Bible, Old and New Testament. It is not from the API: it is downloaded from
[magna25/amharic-bible-json](https://github.com/magna25/amharic-bible-json)
(converted from wordproject.org). The Amharic Bible joins some verses in print
("2-3"); the reader labels those as ranges. Psalm titles show as headings.

Translations downloaded before book codes, headings and footnotes were added
still read normally; download them again to get cross-references for unusual
book layouts, headings and footnotes.

## Data

Translations are stored as JSON files in the user data directory. The file
shape matches the Rust structs:

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
sorted by name. Small sample files for the tests live in
`crates/core/tests/fixtures` and are not loaded by the app.

### Storage locations

| What | Default path | Override |
|---|---|---|
| Downloaded translations | `$XDG_DATA_HOME/oma-bible` | `OMA_BIBLE_DOWNLOAD_DIR` |
| Settings | `$XDG_CONFIG_HOME/oma-bible/settings.json` | `OMA_BIBLE_CONFIG_DIR` |
| Bookmarks and notes | `$XDG_STATE_HOME/oma-bible/bookmarks.json` | `OMA_BIBLE_STATE_DIR` |
| Verse highlights (GPUI) | `$XDG_STATE_HOME/oma-bible/highlights.json` | `OMA_BIBLE_STATE_DIR` |
| Study notes (GPUI) | `$XDG_STATE_HOME/oma-bible/notes.json` | `OMA_BIBLE_STATE_DIR` |
| Cross-references and commentaries | `$XDG_CACHE_HOME/oma-bible/api` | `OMA_BIBLE_CACHE_DIR` |

`OMA_BIBLE_DATA_DIR` also redirects downloads so a single override can point the
whole app at a test library.

## Checks

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Desktop frontend (GPUI)

The desktop UI is built with **GPUI** and `gpui-kit/components` for a
GPU-accelerated, Rust-native experience. The `oma-bible-gpui` crate is the
primary desktop frontend; the shared `oma-bible-core` crate keeps data, themes,
downloads, settings, and bookmarks consistent across the TUI and GPUI apps.

The GTK frontend has been removed from the workspace as part of the switch.
