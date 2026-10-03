#!/usr/bin/env python3
"""Convert SILE's language data into rile/languages/: Liang hyphenation
patterns (languages/*/hyphens*.lua) as .pat files and Fluent messages
(languages/*/messages.ftl) as .ftl files, plus the table that embeds them
(rile/src/language_data.rs).

Usage: scripts/import-sile-languages.py <path to a SILE checkout>

A .pat file is line based: `hyphenmins <left> <right>`, `from <lang>`
(start from another language's data), then `patterns` and `exceptions`
sections with one entry per line.
"""

import pathlib
import re
import sys


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


def main():
    sile = pathlib.Path(sys.argv[1])
    out = pathlib.Path(__file__).resolve().parent.parent / "rile" / "languages"
    out.mkdir(exist_ok=True)
    for old in [*out.glob("*.pat"), *out.glob("*.ftl")]:
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
    codes = {p.stem for p in out.iterdir()}
    print(f"{len(codes)} languages written to {out}")


if __name__ == "__main__":
    main()
