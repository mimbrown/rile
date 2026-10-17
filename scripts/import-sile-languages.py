#!/usr/bin/env python3
"""Convert SILE's language data into rile/languages/: Liang hyphenation
patterns (languages/*/hyphens*.lua) as .pat files and Fluent messages
(languages/*/messages.ftl) as .ftl files, plus the table that embeds them
(rile/src/language_data.rs).

Usage: scripts/import-sile-languages.py <SILE checkout> <tex-hyphen checkout>

SILE's patterns are hyph-utf8's (github.com/hyphenation/tex-hyphen) and a
few others, with their copyright and licence headers removed. SOURCES names
each language's original; rile/languages/NOTICE.md is written from those
originals' headers. Languages in EXCLUDED get no patterns, because their
only licences are copyleft or there is none.

A .pat file is line based: `hyphenmins <left> <right>`, `from <lang>`
(start from another language's data), then `patterns` and `exceptions`
sections with one entry per line.
"""

import pathlib
import re
import sys

# The licence rile takes each language's patterns under, where the original
# offers a choice, and the hyph-utf8 file they come from (or None, with the
# notice in OTHER_NOTICES).
SOURCES = {
    "af": ("hyph-af", "MIT"),
    "as": ("hyph-as", "MIT"),
    "be": ("hyph-be", "MIT"),
    "bg": ("hyph-bg", "BSD-style (see the notice)"),
    "bn": ("hyph-bn", "MIT"),
    "ca": ("hyph-ca", "LPPL"),
    "cop": ("hyph-cop", "MIT"),
    "cu": ("hyph-cu", "MIT"),
    "cy": ("hyph-cy", "MIT"),
    "da": ("hyph-da", "MIT"),
    "de": (None, "LPPL"),
    "el": ("hyph-grc", "MIT"),
    "el-monoton": ("hyph-el-monoton", "MIT"),
    "el-polyton": ("hyph-el-polyton", "MIT"),
    "en": (None, "Knuth's hyphen.tex terms"),
    "eo": (None, "LPPL"),
    "es": ("hyph-es", "MIT"),
    "et": ("hyph-et", "MIT"),
    "eu": ("hyph-eu", "Unicode-style data licence (see the notice)"),
    "fi": ("hyph-fi", "may be freely distributed"),
    "fr": ("hyph-fr", "MIT"),
    "fur": ("hyph-fur", "MIT"),
    "ga": ("hyph-ga", "MIT"),
    "gl": ("hyph-gl", "Unlicense"),
    "gu": ("hyph-gu", "MIT"),
    "hi": ("hyph-hi", "MIT"),
    "hr": ("hyph-hr", "LPPL"),
    "hsb": ("hyph-hsb", "LPPL"),
    "hu": ("hyph-hu", "MPL-1.1"),
    "ia": ("hyph-ia", "LPPL"),
    "is": ("hyph-is", "LPPL"),
    "it": ("hyph-it", "MIT"),
    "ka": ("hyph-ka", "MIT"),
    "kk": ("hyph-kk", "public domain"),
    "kmr": ("hyph-kmr", "LPPL"),
    "kn": ("hyph-kn", "MIT"),
    "la": ("hyph-la", "MIT"),
    "lt": ("hyph-lt", "MIT"),
    "ml": ("hyph-ml", "MIT"),
    "mr": ("hyph-mr", "MIT"),
    "nl": ("hyph-nl", "MIT"),
    "no": ("hyph-no", "all-permissive (see the notice)"),
    "oc": ("hyph-oc", "MIT"),
    "or": ("hyph-or", "MIT"),
    "pa": ("hyph-pa", "MIT"),
    "pi": ("hyph-pi", "MIT"),
    "pl": ("hyph-pl", "MIT"),
    "pt": ("hyph-pt", "BSD-3-Clause"),
    "rm": ("hyph-rm", "MIT"),
    "ru": ("hyph-ru", "LPPL"),
    "sa": ("hyph-sa", "free to use, copy, modify and distribute"),
    "sk": ("hyph-sk", "MIT"),
    "sl": ("hyph-sl", "MIT"),
    "sq": ("hyph-sq", "MIT"),
    "sv": ("hyph-sv", "LPPL"),
    "ta": ("hyph-ta", "MIT"),
    "te": ("hyph-te", "MIT"),
    "th": ("hyph-th", "LPPL"),
    "tk": ("hyph-tk", "MIT"),
    "tr": ("hyph-tr", "LPPL"),
    "uk": ("hyph-uk", "MIT"),
}

# Same patterns as another language: nb and nn are Norwegian, oc-aranes is
# Occitan.
SAME_AS = {"nb": "no", "nn": "no", "oc-aranes": "oc"}

EXCLUDED = {
    "cs": "GPL-2.0-or-later",
    "hy": "LGPL-3.0",
    "id": "GPL-2.0",
    "lv": "LGPL-2.1 or GPL-2.0-or-later",
    "mk": "GPL",
    "ro": "no licence given",
    "sr": "GPL (from hyph-sr-cyrl)",
}

OTHER_NOTICES = {
    "de": """German Hyphenation Patterns (Reformed Orthography, 2006) `dehyphn-x' 2014-05-21 (WL)

From dehyph-exptl, the experimental German hyphenation patterns by Werner
Lemberg and others (https://ctan.org/pkg/dehyph-exptl), released under the
LaTeX Project Public License (https://www.latex-project.org/lppl/); its data
is MIT licensed in later releases.""",
    "en": """The Plain TeX hyphenation tables, hyphen.tex, by Donald E. Knuth and
Frank M. Liang.

The Plain TeX hyphenation tables [NOT TO BE CHANGED IN ANY WAY!]
Unlimited copying and redistribution of this file are permitted as long
as this file is not modified. Modifications are permitted, but only if
the resulting file is not named hyphen.tex.""",
    "eo": """Original file by Fredrick R. Brennan (copypaste@kittens.ph) in 2022, for SILE.
The patterns are from Sergio Pokrovskij's vortdivido project
(https://github.com/pok49/vortdivido, eohyph.m4), usable under the LaTeX
Project Public License (https://www.latex-project.org/lppl/).""",
}

HEADER_KEYS = {"title", "copyright", "notice", "version", "authors", "licence", "source"}


def tokens(src):
    i, n = 0, len(src)
    while i < n:
        c = src[i]
        if c.isspace():
            i += 1
        elif src.startswith("--", i):
            m = re.match(r"--\[(=*)\[", src[i:])
            if m:
                close = "]" + m.group(1) + "]"
                i = src.index(close, i + m.end()) + len(close)
            else:
                j = src.find("\n", i)
                i = n if j < 0 else j + 1
        elif c in "\"'":
            j, out = i + 1, []
            while src[j] != c:
                if src[j] == "\\":
                    j += 1
                    out.append({"n": "\n", "t": "\t"}.get(src[j], src[j]))
                else:
                    out.append(src[j])
                j += 1
            yield ("str", "".join(out))
            i = j + 1
        elif c in "{}=,;":
            yield (c, c)
            i += 1
        else:
            m = re.match(r"[A-Za-z_][A-Za-z0-9_]*|-?\d+(\.\d+)?", src[i:])
            if not m:
                raise ValueError(f"unexpected {src[i:i + 20]!r}")
            text = m.group(0)
            yield ("num", int(text)) if text[0].isdigit() or text[0] == "-" else ("name", text)
            i += m.end()


def parse(src):
    toks = tokens(src)
    kind, value = next(toks)
    assert (kind, value) == ("name", "return"), "expected `return {`"
    assert next(toks)[0] == "{"
    return table(toks)


def table(toks):
    fields, items = {}, []
    for kind, value in toks:
        if kind == "}":
            return fields or items
        if kind in ",;":
            continue
        if kind == "name":
            assert next(toks)[0] == "="
            fields[value] = item(toks, next(toks))
        else:
            items.append(item(toks, (kind, value)))
    raise ValueError("unterminated table")


def item(toks, tok):
    kind, value = tok
    return table(toks) if kind == "{" else value


def minima(data):
    mins = data.get("hyphenmins") or {}
    chosen = mins.get("typesetting") or mins.get("generation")
    if chosen:
        return chosen.get("left", 2), chosen.get("right", 2)
    return None


def write(path, data, keep_minima=True, base=None):
    lines = []
    if base:
        lines.append(f"from {base}")
    mins = minima(data) if keep_minima else None
    if mins:
        lines.append(f"hyphenmins {mins[0]} {mins[1]}")
    for section in ("patterns", "exceptions"):
        entries = data.get(section) or []
        if entries:
            lines.append(section)
            lines.extend(entries)
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def header(path):
    """The copyright, authorship and licence part of a hyph-utf8 file's header."""
    out, keep = [], False
    for line in path.read_text(encoding="utf-8").splitlines():
        if not line.startswith("%"):
            break
        key = re.match(r"% ([a-z_]+):", line)
        if key:
            keep = key.group(1) in HEADER_KEYS
        if keep:
            out.append(line[2:] if line.startswith("% ") else line[1:])
    return "\n".join(out).rstrip()


def notices(path, tex_hyphen, codes):
    commit = (tex_hyphen / ".git" / "HEAD").read_text().strip()
    ref = tex_hyphen / ".git" / commit[5:] if commit.startswith("ref: ") else None
    if ref and ref.exists():
        commit = ref.read_text().strip()
    tex = tex_hyphen / "hyph-utf8/tex/generic/hyph-utf8/patterns/tex"
    lines = [
        "# Hyphenation patterns",
        "",
        "The `.pat` files here are hyphenation patterns converted from SILE's",
        "copies (`languages/*/hyphens*.lua`), which SILE converted from the",
        "originals below with their headers removed. Each file is modified from",
        "its original: reformatted, with comments removed. Each keeps its own",
        "licence, which is not rile's. Where an original offers a choice of",
        "licences, rile takes the one named. The headers are copied from",
        f"hyph-utf8 (https://github.com/hyphenation/tex-hyphen, commit `{commit[:12]}`).",
        "",
        "These languages' patterns are left out, because their only licences are",
        "copyleft or there is none; text in them is not hyphenated:",
        "",
    ]
    lines += [f"- `{c}`: {why}" for c, why in sorted(EXCLUDED.items())]
    lines += [
        "",
        "The LaTeX Project Public License and the Mozilla Public License 1.1, which",
        "some of them are under, are in `LICENSES/`.",
        "",
        "The `.ftl` files are SILE's own, under SILE's MIT licence (`LICENSE`).",
        "",
    ]
    for code in codes:
        if code in SAME_AS:
            lines += [f"## `{code}`", "", f"The patterns of `{SAME_AS[code]}`.", ""]
            continue
        source, licence = SOURCES[code]
        text = OTHER_NOTICES[code] if source is None else header(tex / f"{source}.tex")
        origin = "" if source is None else f", from `{source}.tex`"
        lines += [f"## `{code}`{origin}", "", f"Taken under: {licence}.", "", "```text", text, "```", ""]
    path.write_text("\n".join(lines), encoding="utf-8")


def main():
    sile = pathlib.Path(sys.argv[1])
    tex_hyphen = pathlib.Path(sys.argv[2])
    out = pathlib.Path(__file__).resolve().parent.parent / "rile" / "languages"
    out.mkdir(exist_ok=True)
    for old in [*out.glob("*.pat"), *out.glob("*.ftl"), *out.glob("NOTICE.md")]:
        old.unlink()

    registered = {}
    for init in sorted((sile / "languages").glob("*/init.lua")):
        src = init.read_text(encoding="utf-8")
        req = re.search(r'^local hyphens = require\("languages\.([\w-]+)\.(hyphens[\w-]*)"\)', src, re.M)
        reg = re.search(r'^SILE\.hyphenator\.languages\["([\w-]+)"\] = hyphens', src, re.M)
        if req and reg:
            registered[reg.group(1)] = (req.group(1), req.group(2))

    def load(lang, name):
        return parse((sile / "languages" / lang / f"{name}.lua").read_text(encoding="utf-8"))

    for code, (lang, name) in registered.items():
        if code not in EXCLUDED:
            write(out / f"{code}.pat", load(lang, name))

    # SILE's no/nb/nn register the Norwegian patterns without their minima,
    # and nn adds its own exceptions (languages/nn/init.lua).
    write(out / "no.pat", load("no", "hyphens-tex"), keep_minima=False)
    write(out / "nb.pat", {}, base="no")
    nn = re.search(r"insertvalues\(nn_exceptions, \{(.*?)\}\)", (sile / "languages/nn/init.lua").read_text(), re.S)
    write(out / "nn.pat", {"exceptions": re.findall(r'"([^"]+)"', nn.group(1))}, base="no")
    write(out / "eo.pat", load("eo", "hyphens"))

    for ftl in sorted((sile / "languages").glob("*/messages.ftl")):
        (out / f"{ftl.parent.name}.ftl").write_text(ftl.read_text(encoding="utf-8"), encoding="utf-8")

    pats = sorted(p.stem for p in out.glob("*.pat") if p.read_text(encoding="utf-8").strip())
    unknown = [c for c in pats if c not in SOURCES and c not in SAME_AS]
    assert not unknown, f"no source recorded for {unknown}: add them to SOURCES or EXCLUDED"
    notices(out / "NOTICE.md", tex_hyphen, pats)

    def table(name, ext):
        codes = sorted(p.stem for p in out.glob(f"*.{ext}"))
        arms = "".join(f'        "{c}" => include_str!("../languages/{c}.{ext}"),\n' for c in codes)
        return (
            f"pub(crate) fn {name}(lang: &str) -> Option<&'static str> {{\n"
            "    Some(match lang {\n" + arms + "        _ => return None,\n    })\n}\n"
        )

    (out.parent / "src" / "language_data.rs").write_text(
        "// Generated by scripts/import-sile-languages.py from SILE's languages/.\n\n"
        + table("hyphenation", "pat") + "\n" + table("messages", "ftl"),
        encoding="utf-8",
    )
    codes = {p.stem for p in [*out.glob("*.pat"), *out.glob("*.ftl")]}
    print(f"{len(codes)} languages written to {out}")


if __name__ == "__main__":
    main()
