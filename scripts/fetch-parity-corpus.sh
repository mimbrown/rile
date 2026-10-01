#!/usr/bin/env bash
# Fetch SILE's regression tests and the fonts they need into .parity/.
# Pinned so that parity scores only move when our code does.
set -euo pipefail

SILE_REPO=https://github.com/sile-typesetter/sile.git
SILE_COMMIT=8372c7c6a3b5e4403beb93e62167ad22d514fb9b

root="$(cd "$(dirname "$0")/.." && pwd)/.parity"
mkdir -p "$root/fonts" "$root/downloads"

if [ ! -d "$root/sile/.git" ]; then
  git init -q "$root/sile"
  git -C "$root/sile" remote add origin "$SILE_REPO"
  git -C "$root/sile" sparse-checkout set tests packages/lorem LICENSE.md
fi
git -C "$root/sile" fetch -q --depth 1 origin "$SILE_COMMIT"
git -C "$root/sile" checkout -q FETCH_HEAD

fetch() { # url sha256 file
  local out="$root/downloads/$3"
  if [ ! -f "$out" ] || ! echo "$2  $out" | sha256sum -c --status; then
    curl -fsSL "$1" -o "$out"
    echo "$2  $out" | sha256sum -c --quiet
  fi
}

# SILE's expectations were generated with Gentium Plus 5.000, which only
# SIL's own site serves, so it is kept in the repo (OFL, unmodified).
rm -rf "$root/fonts/gentium-plus"
cp -r "$(dirname "$0")/../sile-parity/fonts/gentium-plus-5.000" "$root/fonts/gentium-plus"

fetch https://github.com/silnrsi/font-gentium/releases/download/v7.000/GentiumBook-7.000.zip \
  fa4e35bcea62dd68befabf4bb7c2765aacd2691f51ec8ae008f5f913ef49f419 GentiumBook-7.000.zip
fetch https://github.com/alerque/libertinus/releases/download/v7.050/Libertinus-7.050.tar.zst \
  cbb54c4c482376eb17bb6397494489baacff0755d3864f9b5c772e2f3d43d429 Libertinus-7.050.tar.zst

unzip -qjo "$root/downloads/GentiumBook-7.000.zip" 'GentiumBook-7.000/GentiumBook-*.ttf' 'GentiumBook-7.000/OFL.txt' -d "$root/fonts/gentium-book"
mkdir -p "$root/fonts/libertinus"
zstd -dcq "$root/downloads/Libertinus-7.050.tar.zst" | tar -x -C "$root/fonts/libertinus" --strip-components 3 --wildcards 'Libertinus-7.050/static/OTF/*.otf'

echo "SILE tests: $(ls "$root/sile/tests"/*.expected | wc -l) expectations at ${SILE_COMMIT:0:10}"
echo "Fonts: $(find "$root/fonts" -name '*.[ot]tf' | wc -l) files in $root/fonts"
