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

# Fonts SILE's tests use, from the sources pinned in SILE's Makefile-fonts.
# SIL's site isn't reachable from here, so SIL fonts come from their GitHub
# releases. SBL Hebrew only comes from SBL's site; when that fails, drop
# SBL_Hbrw.ttf into .parity/fonts/local (any fonts there are used too).
noto=https://raw.githubusercontent.com/googlefonts/noto-fonts/v20201206-phase3/hinted/ttf
font() { # url sha256 file
  fetch "$1" "$2" "$3"
  mkdir -p "$root/fonts/extra"
  cp "$root/downloads/$3" "$root/fonts/extra/$3"
}
font $noto/NotoSansKannada/NotoSansKannada-Regular.ttf \
  51706e1b1de1ec7e2de87bdefb6e47b301b1972d825f027c260a25825b7903a3 NotoSansKannada-Regular.ttf
font $noto/NotoNaskhArabic/NotoNaskhArabic-Regular.ttf \
  c07c44b165c07288c8bb30b0e05f4cf68bef3f52f394c30873f0e731b0698f21 NotoNaskhArabic-Regular.ttf
font $noto/NotoSansMalayalam/NotoSansMalayalam-Regular.ttf \
  68a24696d0c7ef3915eaec061a6dd460d219b2dfa2958cfe4207901ece3c622d NotoSansMalayalam-Regular.ttf
font $noto/NotoSansEthiopic/NotoSansEthiopic-Regular.ttf \
  269b3a54ab56f53e74741f6145d841441cdd97148f3c09377ee4babe472e49c1 NotoSansEthiopic-Regular.ttf
font https://raw.githubusercontent.com/googlefonts/noto-cjk/v20201206-cjk/NotoSansCJK-Regular.ttc \
  d18a3adba5c891eb7f9cdf8c31dad28dcc324db80e662d6a0f91e8db6aee5913 NotoSansCJK-Regular.ttc
font https://raw.githubusercontent.com/googlefonts/noto-cjk/v20201206-cjk/NotoSerifCJK-Regular.ttc \
  fca4e00f179520aca910b246b70d6c9d388e219048281e5e5221860b42c93dab NotoSerifCJK-Regular.ttc
font https://raw.githubusercontent.com/aliftype/amiri/0.113/Amiri-Regular.ttf \
  6f909d81f17de2919be4ce99fac51b07623794d9a46cfd1df53e2d6e669d9263 Amiri-Regular.ttf
font https://raw.githubusercontent.com/aliftype/amiri/0.113/AmiriQuran.ttf \
  4b6042256d04ed5a63cf9aa24c326e1e8f52497d90672c98390ae85079820e9a AmiriQuran.ttf
font https://raw.githubusercontent.com/ChiefMikeK/ttf-symbola/master/Symbola-13.otf \
  bef6770363811144e44970fac4cbf20ae957ceec06a744b06843a9f15895fffe Symbola.otf
font "https://raw.githubusercontent.com/Etcetera-Type-Co/Tourney/643f1026ad6d41b4527f42cd93c776414fdd6503/fonts/variable/Tourney%5Bwdth%2Cwght%5D.ttf" \
  ee686a1a657ca9397d4d1e9ba3ada1c10a5d9b6da53aeb94411cd62b1b793377 "Tourney[wdth,wght].ttf"
font https://github.com/mozilla/twemoji-colr/releases/download/v0.5.1/TwemojiMozilla.ttf \
  860b69e096e5805015cf5b5d64e4ece06c5b987dc05da1f97835c79d9cc79b10 TwemojiMozilla.ttf
font https://raw.githubusercontent.com/ctrlcctrlv/FRBTaiwaneseKana/5c367e9ee5aefd54b5c9c9e996705f0561fe3d15/FRBTaiwaneseKana.otf \
  5bb824c8560e4c25c4a58be3a6f82efae51c709acf1ce7504bf8bf88109f930e FRBTaiwaneseKana.otf
fetch https://github.com/silnrsi/font-awami/releases/download/v2.200/AwamiNastaliq-2.200.zip \
  455202e10883c7ef3d9ee14a96ec75646ee69d5e9dc0a2f70fa7be5f332cd9af AwamiNastaliq-2.200.zip
unzip -qjo "$root/downloads/AwamiNastaliq-2.200.zip" 'AwamiNastaliq-2.200/AwamiNastaliq-Regular.ttf' -d "$root/fonts/extra"
fetch https://github.com/silnrsi/font-lateef/releases/download/v1.200/LateefGR-1.200.zip \
  ef6c6b4b4cb8d8502c78efc43cb99d29f391e6cb9eba1b133a661ffda3ddc7e1 LateefGR-1.200.zip
unzip -qjo "$root/downloads/LateefGR-1.200.zip" 'LateefGR-1.200/LateefGR-Regular.ttf' -d "$root/fonts/extra"
font https://www.sbl-site.org/wp-content/themes/basket/fonts/bib-fonts/SBL_Hbrw.ttf \
  98eca8ecc97af984e205c282d6a0e994af41612029e49a223e85677b71cf9e99 SBL_Hbrw.ttf \
  || echo "SBL Hebrew not fetched; tests using it stay unsupported unless it is in .parity/fonts/local" >&2
fetch https://github.com/CatharsisFonts/Cormorant/releases/download/v3.601/Cormorant_Install_v3.601.zip \
  59997266f48655f7365c0de16d2c4ea6e92a7d8f549fd97c95f1cbe307901e1e Cormorant_Install_v3.601.zip
unzip -qjo "$root/downloads/Cormorant_Install_v3.601.zip" \
  'Cormorant_Install_v3.601/1. TrueType Font Files/CormorantInfant-Regular.ttf' \
  'Cormorant_Install_v3.601/1. TrueType Font Files/CormorantInfant-Italic.ttf' -d "$root/fonts/extra"
fetch https://github.com/source-foundry/Hack/releases/download/v3.003/Hack-v3.003-ttf.tar.xz \
  d9ed5d0a07525c7e7bd587b4364e4bc41021dd668658d09864453d9bb374a78d Hack-v3.003-ttf.tar.xz
tar -xJf "$root/downloads/Hack-v3.003-ttf.tar.xz" -C "$root/fonts/extra" ./Hack-Regular.ttf
fetch https://github.com/google/roboto/releases/download/v2.138/roboto-unhinted.zip \
  70f64c718510a601fbcf752aafe644314dacaeb85474dc689c89787c4a72a728 roboto-unhinted.zip
unzip -qjo "$root/downloads/roboto-unhinted.zip" RobotoCondensed-Bold.ttf -d "$root/fonts/extra"

echo "SILE tests: $(ls "$root/sile/tests"/*.expected | wc -l) expectations at ${SILE_COMMIT:0:10}"
echo "Fonts: $(find "$root/fonts" -name '*.[ot]tf' | wc -l) files in $root/fonts"
