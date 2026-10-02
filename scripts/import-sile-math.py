#!/usr/bin/env python3
"""Convert SILE's math operator dictionary (packages/math/mathml-entities.lua,
itself generated from the W3C's unicode.xml) into sile-core/math/operators.txt.

Usage: scripts/import-sile-math.py <path to a SILE checkout>

One symbol per line, tab separated: the symbol's code points in hex, its
atom type, its TeX-like name (or -), then one field per operator form:
`form lspace rspace flags`, where flags hold s (stretchy), l (largeop) and
m (movablelimits).
"""

import pathlib
import re
import sys

ADD = re.compile(r'addSymbol\(U\(([^)]*)\), "(\w+)", (nil|"[^"]*"), "[^"]*", (nil|\{)')


def forms(src, start):
    depth, i = 1, start
    while depth:
        depth += {"{": 1, "}": -1}.get(src[i], 0)
        i += 1
    out = []
    for block in re.findall(r"\{([^{}]*)\}", src[start:i - 1]):
        props = dict(re.findall(r"(\w+) = ([^,\n]+),", block))
        flags = "".join(f for f, key in (("s", "stretchy"), ("l", "largeop"), ("m", "movablelimits")) if props.get(key) == "true")
        out.append(f'{props["form"].strip(chr(34))} {props.get("lspace", "0")} {props.get("rspace", "0")} {flags}'.rstrip())
    return out, i


def main():
    sile = pathlib.Path(sys.argv[1])
    src = (sile / "packages/math/mathml-entities.lua").read_text()
    lines = []
    pos = 0
    while m := ADD.search(src, pos):
        cps = " ".join(f"{int(c, 16):X}" for c in m.group(1).split(","))
        name = m.group(3).strip('"') if m.group(3) != "nil" else "-"
        fields = [cps, m.group(2), name]
        pos = m.end()
        if m.group(4) == "{":
            f, pos = forms(src, pos)
            fields += f
        lines.append("\t".join(fields))
    out = pathlib.Path(__file__).resolve().parent.parent / "sile-core/math/operators.txt"
    out.parent.mkdir(exist_ok=True)
    out.write_text("\n".join(lines) + "\n")
    print(f"{len(lines)} symbols")


main()
