# Ljós brand

The logo is a mark of two interlocking rounded panels (violet and amber, with a yellow overlap) and the wordmark "Ljós" beside it, with an amber acute accent.

## Files

| File | Use |
|---|---|
| `ljos.svg` | Mark and wordmark for light backgrounds. |
| `ljos-dark.svg` | The same geometry for dark backgrounds. |
| `ljos-mark.svg` | The mark alone, on a square canvas, for icons and avatars. |
| `ljos-mark-dark.svg` | The mark alone for dark backgrounds. |
| `ljos-original.svg` | The first version, kept byte for byte as supplied. Not used anywhere. |

To switch by colour scheme, use `<picture>` as the repository README does:

```html
<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/brand/ljos-dark.svg">
  <img src="docs/brand/ljos.svg" alt="Ljós" width="252">
</picture>
```

## Colours

| Role | Light | Dark |
|---|---|---|
| Back panel (violet) | `#3b1f6e` | `#8a6bd6` |
| Front panel (amber) | `#f49a06` | `#f49a06` |
| Overlap (yellow) | `#ffe000` | `#ffe000` |
| Wordmark | `#2a1650` | `#f1ecfa` |
| Accent (amber) | `#f49a06` | `#f49a06` |

Contrast: violet 13.1:1 on white and 4.8:1 (dark variant) on `#0d0b12`; wordmark 15.8:1 and 16.9:1; amber 8.8:1 on `#0d0b12`. Amber on white is 2.2:1, which is fine for a large graphic but not for text, so never set text in it.

## Changes from the original

Each change is listed with its reason. The original is `ljos-original.svg`.

1. **Violet `#27143f` → `#3b1f6e`.** The original was nearly black and read as black at small sizes; the new one still reads as violet at 16 px.
2. **Wordmark `#211137` → `#2a1650`.** It follows the new violet so the mark and the wordmark share one hue.
3. **Panel corner radius 33 → 36.** Rounder corners keep the shapes soft at the size where the mark is a few pixels wide, and match the overlap's curve.
4. **Wordmark scaled 1.12× and centred on the mark.** The script's x-height was small beside the mark; at 120 px wide it was hard to read. Centring on the mark's midline stops the descender and the accent hanging unevenly.
5. **Gap between mark and wordmark 46 → 34.** The wordmark had drifted away from the mark; the two now read as one lockup.
6. **Accent stroked 3 units.** The thin amber accent vanished at 120 px wide; the stroke keeps it visible without changing its shape.
7. **Canvas trimmed to the artwork plus 16 units on each side**, from an uneven 24 and 29 on the left and right. The logo then sits flush in its box when used at a stated width.
8. **Mark on a square canvas** (`0 -3 188 188`), so it drops into square icon slots without being stretched or off-centre.
9. **Dark variant.** The violet is `#8a6bd6` (4.8:1 on `#0d0b12`; the original violet was 1.2:1) and the wordmark `#f1ecfa`. Amber and yellow are unchanged because they already pass.
10. **`ljos-mark-dark.svg` added.** The mark alone is needed on dark backgrounds too.

Checked at 16 px and 32 px (mark) and 120 px wide (full logo), on white and on `#0d0b12`.

## Wordmark font

The wordmark is set in Birthstone (Google Fonts, SIL Open Font License) and outlined to paths, so no font is needed to render it.

## Naming

Ljós is general purpose. HAL-C2 is one consumer, and its name stays out of the logo, these files and their text.
