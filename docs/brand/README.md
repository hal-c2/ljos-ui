# Ljós brand

The logo is a mark of two interlocking rounded panels, with the wordmark "Ljós" beside it.

## Files

| File | Use |
|---|---|
| `ljos.svg` | Mark and wordmark, 504×240, for light backgrounds. |
| `ljos-dark.svg` | The same geometry for dark backgrounds: light wordmark, lighter violet panel. |
| `ljos-mark.svg` | The mark alone, with a tight viewBox, for icons and avatars. |

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
| Back panel (midnight violet) | `#27143f` | `#7b5cc0` |
| Front panel (amber) | `#f49a06` | `#f49a06` |
| Overlap (yellow) | `#ffe000` | `#ffe000` |
| Wordmark | `#211137` | `#ece6f5` |

The dark variant lightens the violet because `#27143f` is 1.2:1 against `#0d0b12`; `#7b5cc0` is 3.9:1, above the 3:1 a graphic needs. The amber and yellow already pass (8.8:1 and 14.8:1).

## Wordmark font

The wordmark is set in Birthstone (Google Fonts, SIL Open Font License) and outlined to paths, so no font is needed to render it.

## Naming

Ljós is general purpose. HAL-C2 is one consumer, and its name stays out of the logo, these files and their text.
