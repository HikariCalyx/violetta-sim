"""Builds the small CJK fonts the app needs to draw its localised text.

`epaint`'s bundled fonts (Ubuntu, Hack, Noto Emoji) carry no CJK at all, so a
string like `비올레타 거짓말 탐지 시뮬레이터` or `다시하기` would be drawn as replacement
boxes. Embedding a CJK font whole would be 9-17 MB, so this pins one weight and
keeps only the glyphs each language actually uses — tens of KB per language, and
the language's own glyph forms with it, since each Noto Sans CJK covers its own
language's characters.

The strings are read from `src/language.rs` — the app's name and the words it
writes itself — so run this from the repository root whenever one of them changes:

    py scripts/subset-cjk-font.py

It writes `violetta_lie_detector_sim/fonts/<language>.otf` and `fonts/OFL.txt`, the
licence the fonts come with (SIL Open Font License 1.1, which allows this). Pass the
directory holding the fonts as an argument if they are not where Windows keeps them.

`cargo test` checks that every shipped language has a font here covering every string
it draws, so forgetting to re-run this is a failing test rather than a box on screen.
"""

import os
import re
import sys

from fontTools import subset
from fontTools.ttLib import TTFont
from fontTools.varLib import instancer

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
LANGUAGE_RS = os.path.join(REPO, "violetta_lie_detector_sim", "src", "language.rs")
OUT_DIR = os.path.join(REPO, "violetta_lie_detector_sim", "fonts")
WEIGHT = 400

# Which Noto Sans CJK each language is written in. These are the family names
# Windows ships as variable fonts; the Chinese pair differ in glyph forms, which is
# exactly why each language gets its own subset rather than one merged font.
SOURCES = {"cn": "NotoSansSC", "tw": "NotoSansTC", "jp": "NotoSansJP", "kr": "NotoSansKR"}


def localised_strings():
    """Every language's text: its name, and then the words its screens use.

    The two tables this reads are one language to a line, which is what makes this
    parsing easy to keep honest — `language::title` and `language::words`.
    """
    source = open(LANGUAGE_RS, encoding="utf-8").read()
    strings = {}

    for language, name in re.findall(r'Some\("(\w\w)"\) => "([^"]+)",', source):
        strings[language] = name

    for language, arguments in re.findall(r'Some\("(\w\w)"\) => Words::new\(([^)]*)\),', source):
        for word in re.findall(r'"([^"]*)"', arguments):
            strings[language] = strings.get(language, "") + word

    if not strings:
        raise SystemExit(f"found no localised strings in {LANGUAGE_RS}")

    return strings.items()


def licence_of(font):
    """The licence text the font carries in its `name` table (name ID 13)."""
    for record in font["name"].names:
        if record.nameID == 13:
            return record.toUnicode().strip()
    return ""


def main():
    font_dir = sys.argv[1] if len(sys.argv) > 1 else os.path.join(
        os.environ.get("WINDIR", r"C:\Windows"), "Fonts"
    )
    os.makedirs(OUT_DIR, exist_ok=True)

    licence = ""
    for language, text in localised_strings():
        family = SOURCES.get(language)
        if family is None:
            print(f"{language}: no source font known, skipping")
            continue

        path = os.path.join(font_dir, f"{family}-VF.ttf")
        font = TTFont(path)

        missing = [c for c in text if not c.isspace() and ord(c) not in font.getBestCmap()]
        if missing:
            raise SystemExit(f"{path} has no glyph for `{''.join(missing)}`")

        if "fvar" in font:
            font = instancer.instantiateVariableFont(font, {"wght": WEIGHT})

        options = subset.Options()
        # The app draws one glyph after another; there is nothing to shape.
        options.layout_features = []
        subsetter = subset.Subsetter(options=options)
        subsetter.populate(text=text)
        subsetter.subset(font)

        out = os.path.join(OUT_DIR, f"{language}.otf")
        font.save(out)
        print(f"{language}: {text} -> {os.path.basename(out)} ({os.path.getsize(out) / 1024:.1f} KB)")

        licence = licence or licence_of(TTFont(path))

    if licence:
        with open(os.path.join(OUT_DIR, "OFL.txt"), "w", encoding="utf-8") as file:
            file.write(licence + "\n")
            file.write(
                "\nThe full text of the licence is in LICENSE-OFL-1.1.txt, alongside "
                "this file.\n"
            )
        print("wrote OFL.txt")


if __name__ == "__main__":
    main()
