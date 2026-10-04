"""Writes src/data/unicode-names.txt: one `HEX;NAME` line per named
character in UnicodeData.txt, for the 01 sheet's character search.

Skipped: controls (named `<control>`) and the ranges Unicode names
algorithmically (CJK ideographs, Hangul syllables, Tangut, …), which
UnicodeData.txt lists only as `<…, First>` / `<…, Last>` pairs. Those are
still found by codepoint or by pasting the character.

Run from editors/web: python scripts/unicode-names.py
"""

import io
import urllib.request

VERSION = "16.0.0"
URL = f"https://www.unicode.org/Public/{VERSION}/ucd/UnicodeData.txt"
OUT = "src/data/unicode-names.txt"

with urllib.request.urlopen(URL) as response:
    lines = response.read().decode("utf-8").splitlines()

out = io.open(OUT, "w", encoding="utf-8", newline="\n")
out.write(f"# Unicode {VERSION} character names, from {URL}\n")
count = 0
for line in lines:
    cp, name = line.split(";")[:2]
    if name.startswith("<"):
        continue
    out.write(f"{cp};{name}\n")
    count += 1
out.close()
print(f"{count} names -> {OUT}")
