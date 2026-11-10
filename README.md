# Oma Bible

Your own quiet place to read Scripture. A native Rust Bible reader for Linux,
desktop and terminal, with Hebrew & Greek word study, cross-references,
commentary and right-to-left scripts. It follows your Omarchy theme live. The
King James Version is built in, so it works offline from the first start.

**Oma** is Estonian for *one's own*, as in *oma kodu*, "one's own home".
Oma Bible is meant to feel like that: nothing on the page but Scripture until
you reach for a study tool. The name also nods to Omarchy, the desktop it
grew up on.

The interface follows the quiet, document-first look of
[Omawrite](https://github.com/omacom/omawrite): theme colours, restrained
chrome, a centred reading column, and a muted footer.

## Workspace

| Crate | Purpose |
|---|---|
| `crates/core` | Bible data model, JSON loading, HelloAO downloads, themes, persistence (`oma-bible-core`) |
| `crates/tui` | Terminal reader (`oma-bible-tui`) |
| `crates/bundle` | Rebuilds the built-in KJV data packs (`oma-bible-bundle`) |
| `crates/gpui` | GPU-accelerated desktop app (package `oma-bible-gpui`, binary `oma-bible`; GPUI + gpui-kit/components) |

The core crate has no UI dependencies, so front ends share the same models,
loaders, download client, settings, bookmarks, and theme catalog.

## Install

**Arch / Omarchy (AUR):**

```bash
yay -S oma-bible-bin       # desktop app
yay -S oma-bible-tui-bin   # terminal app
```

**Any Linux:** the [Releases](../../releases) page has one download per app.
Take one or both; they work on their own and share your library, bookmarks and
settings when both are installed.

| Download | Program | What it is |
|---|---|---|
| `oma-bible-<version>-x86_64-linux.tar.gz` | `oma-bible` | The desktop app (GPUI) |
| `oma-bible-tui-<version>-x86_64-linux.tar.gz` | `oma-bible-tui` | The terminal app |

```bash
tar -xzf oma-bible-0.1.1-x86_64-linux.tar.gz
cd oma-bible-0.1.1-x86_64-linux
./install.sh
```

`install.sh` puts the program in `~/.local/bin` and adds it to the app launcher;
no `sudo` needed. The King James Version and its study data are built into each
app, so it works offline straight away.

## Run from source

Desktop (binary `oma-bible`):

```bash
cargo run -p oma-bible-gpui
```

Terminal (binary `oma-bible-tui`):

```bash
cargo run -p oma-bible-tui
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
| `w` / double-click a word | Original Hebrew or Greek words of the verse, with the lexicon (GPUI) |
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
copy, study verse, chapters, and settings. `Ctrl+Shift+D` (or the Dock row in settings) moves it to the
left, top, or right edge, or hides it; the choice is saved.

Right-click anywhere in the text to open the action wheel around the pointer;
the verse under the pointer becomes the selected one. `Space` opens it in the
middle of the window. The wheel is a ring cut into seven segments with a hole
in the middle; pointing anywhere in a segment picks it, and a label under the
ring names the picked tool and its key. Circle the ring with `l` / `j` / `→` /
`↓` / `Tab` (clockwise) or `h` / `k` / `←` / `↑` / `Shift+Tab`, then press `Enter`.
`1`–`7` or a tool's own key (`o`, `m`, `M`, `H`, `y`, `r`, `S`) runs one
directly. `Esc`, `Space` or a click in the hole closes it.

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

The study sheet (`r`, `c`, or **Study verse** on the wheel) has four tabs:
**Cross-references** from the open-cross-ref dataset, strongest first, shown in
the open translation (click one or press `Enter` to go there); **Commentary**
from one of the API's commentaries (Matthew Henry by default; `c` or the
commentary button switches, and **Settings → Commentary** opens a dropdown of
every commentary; the choice is saved), showing the section that
covers the verse; **Footnotes** from the translation itself; and **Original**,
the verse word by word in Hebrew or Greek. `Tab`, `h` / `l` or `1`–`4` switch
tabs. Study data is fetched once per chapter and cached in
`$XDG_CACHE_HOME/oma-bible` for offline use. Section headings from the
translation appear above the verses they introduce.

### Original Hebrew and Greek words

Double-click a word in the reader (or press `w` for the selected verse) to open
the **Original** tab. It lists the verse's words in Hebrew (right to left) or
Greek, each with its transliteration and English gloss. The picked word shows
its Strong's number, full parsing ("Verb – Qal – Perfect – third person
masculine singular") and its lexicon entry with every sense. Click a word or
press `w` / `b` to move to the next or previous one.

A double-clicked word is linked to its original word like this:

- **English translations**: the word is matched to the interlinear's English
  glosses (`heaven` finds הַשָּׁמַיִם "the heavens"). When a word appears more
  than once, the one at the same place in the sentence wins. An older word with
  no matching gloss (KJV "firmament", BSB "expanse") falls back to the word at
  the same place.
- **Hebrew and Greek translations**: matched on the original text, ignoring
  vowel points and accents.
- **Other languages** (Amharic, Arabic and so on): the sheet opens on the verse
  and asks you to pick the word. Without word-alignment data for those
  translations, a guess would be wrong more often than right.

The words come from the Berean Standard Bible interlinear (public domain) and
the definitions from the STEPBible brief lexicons (TBESH and TBESG,
STEPBible.org / Tyndale House, CC BY 4.0), both served by the Free Use Bible
API. Each chapter's words and each lexicon entry are fetched once and cached.

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

Open the theme screen with `T` (or **Settings → Theme**).

**On Omarchy** the first row is **Follow Omarchy**, the default. While it is
on, both apps use the current Omarchy theme and redraw within a second when you
run `omarchy theme set <name>`, with no restart. Pick any other theme to use it
just in Oma Bible; the desktop's changes are then ignored until you pick
**Follow Omarchy** again. The choice is saved.

**Off Omarchy** every row is a theme, and the one you pick is saved:

- **TUI:** opens the theme picker at startup until a theme has been picked.
- **GPUI:** until a theme is picked, starts with a palette matching the system
  light/dark appearance and follows it.

Themes are discovered from:

1. `~/.config/omarchy/themes/*/colors.toml` — user themes
2. `$OMARCHY_PATH/themes/*/colors.toml` — stock themes
3. `/usr/share/omarchy/themes/*/colors.toml` — stock fallback

User themes win when a slug exists in both places. `Oma Dark` and `Oma Light`
are always available, so a machine without Omarchy still has a dark and a
light choice.

## Bible API downloads

The **Add a translation** screen reads the catalogue from
[bible.helloao.org](https://bible.helloao.org/) and lists every available
translation, grouped and filterable by language. Downloads are streamed to disk
and can be cancelled at any time. In the desktop app a toolbar above the list has a
language button (same as `l`), a search field (same as `/`, with a clear
button) and Refresh (same as `r`).

- Catalogue: `GET /api/available_translations.json`
- Translation: `GET /api/{id}/complete.simple.json`
- Downloads are saved as JSON in the user data directory. The King James
  Version is built in (see below), so a fresh install opens on it; a
  downloaded `eng_kjv` replaces the built-in copy.
- The language filter starts on English and then remembers the last choice.

Downloaded translations survive restarts. The reader shows exactly what the API
provides; some API translations are incomplete (for example `amh_amh`, shown as
**አዲስ ኪዳን**, is the New Testament only and is missing verses in the source
data).

The catalogue also lists **መጽሐፍ ቅዱስ** (`amh_full`), the complete Amharic
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

## Built-in data

The King James Version and its study data are built into both apps, so they
work offline from the first start:

| Pack (`crates/core/bundle/`) | Contents |
|---|---|
| `kjv.pack` | King James Version |
| `cross-references.pack` | open-cross-ref cross-references for every chapter |
| `commentary.pack` | Matthew Henry's commentary (the default) and the commentary list |
| `interlinear.pack` | BSB interlinear: the original Hebrew and Greek words |
| `lexicon.pack` | STEPBible brief lexicons (TBESH, TBESG) |

Each pack is an index plus separately compressed entries, so opening a chapter
inflates only that chapter. Study lookups try the built-in packs first, then
the disk cache, then the API, so other commentaries still download on demand.

To refresh the packs from the Free Use Bible API (about 21,700 requests, six
at a time; raw downloads are kept in `target/bundle-cache` so a stopped run
resumes), then rebuild the apps:

```bash
cargo run --release -p oma-bible-bundle
```

## Releasing

Releases are built by `.github/workflows/release.yml`:

1. Set the version in `Cargo.toml` (`[workspace.package] version`) and commit.
2. Tag and push: `git tag -a v0.2.0 -m "Oma Bible 0.2.0" && git push origin main v0.2.0`.
3. GitHub Actions runs the tests, builds both apps on Ubuntu 22.04 and attaches
   one download per app (`oma-bible-…` and `oma-bible-tui-…`, each with a
   `.sha256`) to a **draft** release with generated notes. The packing is
   `packaging/package.sh`, which also runs locally after `cargo build --release`.
4. Edit the notes on GitHub and press **Publish release**.
5. Update the AUR package from `packaging/aur/PKGBUILD` (see the comments at
   its top).

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

## Credits and licences

Oma Bible's code is released under the [MIT License](LICENSE).

The Bible texts and study data keep their own licences:

| Data | Source | Licence |
|---|---|---|
| King James Version (built in) | [eBible.org](https://ebible.org/Scriptures/details.php?id=eng-kjv2006) via the Free Use Bible API | Public domain |
| Original Hebrew and Greek words (built in) | [Berean Standard Bible](https://berean.bible/) interlinear | Public domain |
| Lexicon (built in) | [STEPBible.org](https://github.com/STEPBible/STEPBible-Data) / Tyndale House, Cambridge: Translators Brief lexicons of Extended Strongs (TBESH, TBESG) | [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/) |
| Cross-references (built in) | [OpenBible.info](https://www.openbible.info/labs/cross-references/) | [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/) |
| Matthew Henry's commentary (built in) | Matthew Henry (1706–1721) via the Free Use Bible API | Public domain |
| Other translations and commentaries | Downloaded on demand from the [Free Use Bible API](https://bible.helloao.org/) | Each one's own; see the API catalogue |
| Complete Amharic Bible (download) | [magna25/amharic-bible-json](https://github.com/magna25/amharic-bible-json), from wordproject.org | MIT (conversion) |

The built-in data was reformatted to fit the app (unused fields removed,
compressed into packs). The desktop app is built on
[GPUI](https://github.com/zed-industries/zed) and gpui-kit (Apache-2.0); the
terminal app on [ratatui](https://ratatui.rs) (MIT). Thanks to everyone who
made these texts and tools free to use.
