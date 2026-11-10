#!/bin/sh
# Packs the release builds into one download per app:
#   oma-bible-<version>-x86_64-linux.tar.gz      desktop app
#   oma-bible-tui-<version>-x86_64-linux.tar.gz  terminal app
# each with a .sha256 file. Usage: packaging/package.sh <version> <out-dir>
# (run from the repository root after `cargo build --release`).
set -eu
version="$1"
out="$2"
mkdir -p "$out"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

for app in oma-bible oma-bible-tui; do
  name="$app-$version-x86_64-linux"
  mkdir "$work/$name"
  cp "target/release/$app" "packaging/$app.desktop" packaging/install.sh \
     README.md LICENSE "$work/$name/"
  tar -czf "$out/$name.tar.gz" -C "$work" "$name"
  (cd "$out" && sha256sum "$name.tar.gz" > "$name.tar.gz.sha256")
done
ls -l "$out"
