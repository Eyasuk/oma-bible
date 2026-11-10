#!/bin/sh
# Installs Oma Bible for the current user: binaries into ~/.local/bin and
# launcher entries into ~/.local/share/applications. Run it from the
# unpacked release folder.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
bin="${XDG_BIN_HOME:-$HOME/.local/bin}"
apps="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
mkdir -p "$bin" "$apps"
install -m 755 "$here/oma-bible" "$here/oma-bible-tui" "$bin/"
install -m 644 "$here/oma-bible.desktop" "$here/oma-bible-tui.desktop" "$apps/"
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$apps" || true
echo "Installed oma-bible and oma-bible-tui to $bin"
