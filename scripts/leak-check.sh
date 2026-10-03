#!/usr/bin/env bash
# Typeset Markdown through the rile CLI under Valgrind and fail on any
# definite or indirect leak. Not run in CI; slow on large documents.
#
#   scripts/leak-check.sh [--harfbuzz] [file.md]
#
# Without a file, a sample with headings, lists, links, code and math is
# used. Fonts come from .parity/fonts (scripts/fetch-parity-corpus.sh).
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
features=()
if [ "${1:-}" = "--harfbuzz" ]; then
  features=(--features rile/harfbuzz)
  shift
fi
command -v valgrind >/dev/null || { echo "valgrind is not installed" >&2; exit 2; }

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
input="${1:-$work/sample.md}"
if [ $# -eq 0 ]; then
  cat > "$input" <<'MD'
# Leak check

A paragraph with *emphasis*, **bold**, `code`, a [link](https://example.org)
and $x^2 + \frac{a}{b} = \sqrt{y_1}$ inline, long enough to need hyphenation
and several lines of justified text in the narrow page.

> A quotation that wraps onto a second line in the block quote.

1. First item
2. Second item with a nested list:
   - nested one
   - nested two

$$\int_0^1 f(x)\,dx = F(1) - F(0)$$

```
fn main() {
    println!("hi");
}
```

## A section

Closing words.
MD
fi

cargo build -q --release -p rile-cli --target-dir "$root/target/leak-check" "${features[@]}"
valgrind --quiet --leak-check=full --show-leak-kinds=definite,indirect \
  --errors-for-leak-kinds=definite,indirect --error-exitcode=1 \
  "$root/target/leak-check/release/rile" --class book --paper a5 \
  --fonts-dir "$root/.parity/fonts" -o "$work/out.pdf" "$input"
echo "No leaks."
