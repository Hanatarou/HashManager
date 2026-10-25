#!/usr/bin/env python3
"""Cut the Noto Sans fonts down to the characters Hash Manager needs and write them to resources/fonts.

usage: python3 tools/subset_fonts.py SOURCE_DIR
SOURCE_DIR holds the full fonts: NotoSans_400Regular.ttf, NotoSansDevanagari_400Regular.ttf, NotoSansBengali_400Regular.ttf,
NotoSansTelugu_400Regular.ttf, NotoSansArabic_400Regular.ttf (Google Fonts; npm: @expo-google-fonts/noto-sans*) and
NotoSansSC_400Regular.ttf, NotoSansJP_400Regular.ttf (Google Fonts; npm: @expo-google-fonts/noto-sans-sc, noto-sans-jp), plus Hack-Regular.ttf and
emoji-icon-font.ttf (both are in the crate epaint_default_fonts, folder fonts/) for the monospaced hashes and the arrows of the menus.
Needs fontTools (pip install fonttools). Run again whenever a language file changes: a character that a language file uses
and a font lacks is drawn as an empty box (tests/all/round2.rs, `every_character_of_every_language_has_a_glyph`, catches it).
A font whose source file is not in SOURCE_DIR is skipped, so one script can be redone without the others.
"""
import glob, os, re, subprocess, sys

root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
src = sys.argv[1]
out = os.path.join(root, "resources", "fonts")
os.makedirs(out, exist_ok=True)

def chars_of(paths):
    s = set()
    for p in paths:
        s |= set(open(p, encoding="utf-8").read())
    return s

lang_files = sorted(glob.glob(os.path.join(root, "lang", "*.txt")))
names = re.findall(r'lang!\("[^"]+", "([^"]+)"', open(os.path.join(root, "src", "i18n.rs"), encoding="utf-8").read())
used = chars_of(lang_files) | set("".join(names))
used = {c for c in used if c not in "\r\n\t"} | set(chr(c) for c in range(0x20, 0x7F))

def ranges(*rs):
    s = set()
    for a, b in rs:
        s |= set(chr(c) for c in range(a, b + 1))
    return s

# Whole writing-system blocks for the scripts whose fonts are small (file names in these scripts show correctly).
latin = ranges((0x20, 0x7E), (0xA0, 0x17F), (0x180, 0x24F), (0x300, 0x36F), (0x370, 0x3FF), (0x400, 0x52F), (0x1E00, 0x1EFF), (0x2000, 0x206F), (0x20A0, 0x20BF), (0x2190, 0x21FF), (0x2200, 0x22FF), (0x25A0, 0x25FF), (0x2713, 0x2717))
deva = ranges((0x900, 0x97F), (0xA8E0, 0xA8FF), (0x1CD0, 0x1CFF))
beng = ranges((0x980, 0x9FF))
telu = ranges((0xC00, 0xC7F))
arab = ranges((0x600, 0x6FF), (0x750, 0x77F), (0x8A0, 0x8FF), (0xFB50, 0xFDFF), (0xFE70, 0xFEFF))
common = ranges((0x20, 0x7E), (0xA0, 0xFF), (0x2000, 0x206F), (0x200C, 0x200F), (0x25CC, 0x25CC))

jobs = [
    ("NotoSans_400Regular.ttf", "NotoSans-subset.ttf", used | latin),
    ("NotoSansDevanagari_400Regular.ttf", "NotoSansDevanagari-subset.ttf", used | deva | common),
    ("NotoSansBengali_400Regular.ttf", "NotoSansBengali-subset.ttf", used | beng | common),
    ("NotoSansTelugu_400Regular.ttf", "NotoSansTelugu-subset.ttf", used | telu | common),
    ("NotoSansArabic_400Regular.ttf", "NotoSansArabic-subset.ttf", used | arab | common),
    ("NotoSansSC_400Regular.ttf", "NotoSansSC-subset.ttf", used),
    ("NotoSansJP_400Regular.ttf", "NotoSansJP-subset.ttf", used),
    # Hack keeps its hinting (the hashes are drawn small); the arrows are the four of a submenu (U+23F4..U+23F7).
    ("Hack-Regular.ttf", "Hack-subset.ttf", ranges((0x20, 0x7E), (0xA0, 0xFF))),
    ("emoji-icon-font.ttf", "Symbols-subset.ttf", ranges((0x23F4, 0x23F7))),
    # The white square egui draws for a character no font has; kept in a font of its own (see src/ui/fonts.rs).
    ("Hack-Regular.ttf", "Replacement-subset.ttf", ranges((0x25FB, 0x25FB))),
]
for srcname, dst, chars in jobs:
    if not os.path.exists(os.path.join(src, srcname)):
        print("skipped (no source):", dst)
        continue
    text = os.path.join(out, "_chars.txt")
    open(text, "w", encoding="utf-8").write("".join(sorted(chars)))
    hinting = [] if dst == "Hack-subset.ttf" else ["--no-hinting"]
    subprocess.check_call(["pyftsubset", os.path.join(src, srcname), "--text-file=" + text, "--layout-features=*", *hinting,
                           "--drop-tables+=DSIG", "--notdef-outline", "--output-file=" + os.path.join(out, dst)])
    print(dst, os.path.getsize(os.path.join(out, dst)))
os.remove(os.path.join(out, "_chars.txt"))
