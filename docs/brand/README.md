# Ljós brand

The logo is a mark of two interlocking rounded panels (violet and amber, with a yellow overlap) and the wordmark "Ljós" beside it, with an amber acute accent.

## Files

| File | Use |
|---|---|
| `ljos.svg` | Mark and wordmark. One file for light and dark backgrounds. |
| `ljos-mark.svg` | The mark alone, on a square canvas, for icons and avatars. |
| `ljos-original.svg` | The first version, kept byte for byte as supplied. Not used anywhere. |

There is no dark variant. Use the file as it is on any background:

```html
<img src="docs/brand/ljos.svg" alt="Ljós" width="252">
```

## Colours

| Role | Colour |
|---|---|
| Back panel (violet) | `#8260da` |
| Front panel (amber) | `#f49a06` |
| Overlap (yellow) | `#ffe000` |
| Wordmark (violet) | `#8260da` |
| Accent (amber) | `#f49a06` |

The violet sits halfway between light and dark: 4.5:1 on white and 4.3:1 on `#0d0b12` (4.2:1 on GitHub's dark `#0d1117`), so the panel and the wordmark read on either without a second file. Amber is 8.8:1 on `#0d0b12` and 2.2:1 on white, which is fine for a large graphic beside the violet but not for text, so never set text in it.

## Changes from the original

Each change is listed with its reason. The original is `ljos-original.svg`.

1. **Violet `#27143f` → `#8260da`.** The original was nearly black: it read as black at small sizes and disappeared on dark backgrounds (1.2:1). The mid-tone violet reads as violet at 16 px and passes 4.3:1 or better on both white and near-black, so one file serves both colour schemes.
2. **Wordmark `#211137` → `#8260da`.** Near-black ink vanishes on a dark page; the wordmark takes the panel's violet, so it reads on both and the mark and the wordmark share one colour.
3. **Panel corner radius 33 → 36.** Rounder corners keep the shapes soft at the size where the mark is a few pixels wide, and match the overlap's curve.
4. **Wordmark scaled 1.12× and centred on the mark.** The script's x-height was small beside the mark; at 120 px wide it was hard to read. Centring on the mark's midline stops the descender and the accent hanging unevenly.
5. **Gap between mark and wordmark 46 → 34.** The wordmark had drifted away from the mark; the two now read as one lockup.
6. **Accent stroked 3 units.** The thin amber accent vanished at 120 px wide; the stroke keeps it visible without changing its shape.
7. **Canvas trimmed to the artwork plus 16 units on each side**, from an uneven 24 and 29 on the left and right. The logo then sits flush in its box when used at a stated width.
8. **Mark on a square canvas** (`0 -3 188 188`), so it drops into square icon slots without being stretched or off-centre.
9. **No dark variant.** Earlier drafts had `-dark` files switched with `<picture>`; the mid-tone violet makes them unnecessary, and one file works where `<picture>` or CSS can't switch (images in issues, chat previews, native apps).
10. **The `<title>` and `<desc>` carry the ids `aria-labelledby` names.** The original pointed at `title desc`, which no element had, so a screen reader announced nothing.

Checked at 16 px and 32 px (mark) and 120 px wide (full logo), on white and on `#0d0b12`.

## Wordmark font

The wordmark is set in Birthstone (Google Fonts, SIL Open Font License) and outlined to paths, so no font is needed to render it.

## Naming

Ljós is general purpose. HAL-C2 is one consumer, and its name stays out of the logo, these files and their text.
