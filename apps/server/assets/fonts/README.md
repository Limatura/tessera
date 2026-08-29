# Fonts embedded for OG images

`og.rs` renders link-preview cards, and a card is drawn on a server that has no
system fonts to borrow — a scratch container has none at all. These five faces
are compiled into the binary so a fresh install draws a correct card before
anyone has configured anything.

Which faces, and why these, is recorded in #9. What follows is what is here and
what it costs; the coverage argument is in the issue.

| File | Size | Covers |
| --- | --- | --- |
| `DejaVuSans.woff2` | 0.25 MB | Latin, Cyrillic, Greek, some Arabic and Hebrew |
| `DejaVuSans-Bold.woff2` | 0.23 MB | the same, bold — every heading on a card is bold |
| `DroidSansFallback.woff2` | 1.41 MB | CJK ideographs, kana, bopomofo |
| `NotoSansKR-hangul.woff2` | 0.95 MB | Hangul, all 11 172 precomposed syllables |
| `NotoEmoji.woff2` | 0.45 MB | 1 503 emoji, monochrome |

Total 3.29 MB.

## Why Korean needs its own file

`DroidSansFallback` reports Hangul support and does not have it: the
Compatibility Jamo block (the individual letters, for keyboard charts) is 97.9%
covered, and Hangul Syllables — how Korean is actually written — is 3 of 11 172.
`한국어` renders as three empty boxes. #9 records this at length because it is
exactly the trap someone will fall into again when they ask why we ship two CJK
files.

## Why the emoji are monochrome

Noto Color Emoji is 9.2 MB as woff2, against a 28 MB budget for the whole
binary. The monochrome face is 0.45 MB and reads correctly on the card's dark
ground. An operator who wants colour can add the font — that path is #7.

## Rebuilding these

They are `woff2` conversions of faces packaged by Debian and Google Fonts,
made with `fontTools`. Nothing is subset except the Korean one, which is Noto
Sans CJK KR reduced to Hangul, the two jamo blocks, and the won sign.

```
pip install fonttools brotli
python - <<'PY'
from fontTools.ttLib import TTFont
f = TTFont("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf")
f.flavor = "woff2"
f.save("DejaVuSans.woff2")
PY
```

## Licences

- DejaVu — see `DejaVu.LICENSE.txt` (Bitstream Vera and Arev, both permissive)
- Droid Sans Fallback — see `DroidSansFallback.LICENSE.txt` (Apache 2.0)
- Noto Sans KR and Noto Emoji — SIL Open Font License 1.1, see `Noto.LICENSE.txt`
