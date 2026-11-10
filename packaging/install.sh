#!/bin/sh
# Installs Oma Bible for the current user: each program found next to this
# script goes into ~/.local/bin and its launcher entry into
# ~/.local/share/applications. The desktop and terminal downloads each carry
# this script, so it installs whichever app you unpacked.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
bin="${XDG_BIN_HOME:-$HOME/.local/bin}"
apps="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
installed=""

for app in oma-bible oma-bible-tui; do
  [ -f "$here/$app" ] || continue
  mkdir -p "$bin" "$apps"
  install -m 755 "$here/$app" "$bin/"
  if [ -f "$here/$app.desktop" ]; then
    install -m 644 "$here/$app.desktop" "$apps/"
  fi
  installed="$installed $app"
done

if [ -z "$installed" ]; then
  echo "No Oma Bible program found next to install.sh" >&2
  exit 1
fi

command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$apps" || true
echo "Installed$installed to $bin"
case ":$PATH:" in
  *":$bin:"*) ;;
  *) echo "Note: $bin is not on your PATH; add it to run the app by name." ;;
esac
