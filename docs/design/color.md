# Colour

Colour carries hierarchy, state and brand, and it is the first thing to break in dark mode or high contrast. The rules check contrast and where colours come from; the theory below is how to choose them.

## Colour for people who are not designers

### Work in OKLCH, not HSL

HSL's lightness is a formula, not what eyes see: `hsl(60 100% 50%)` (yellow) and `hsl(240 100% 50%)` (blue) have the same L and look nothing alike in lightness, so rotating a hue in HSL quietly changes contrast. In OKLCH, L is perceived lightness (0 to 1), C is chroma (how colourful, about 0 to 0.37) and H the hue angle. Two colours with the same L read as equally light whatever their hue, so you can rotate H for a harmony and keep text readable. Caveats: not every L, C, H exists on an sRGB screen (reduce C until it fits, keeping L and H), and L is not a contrast measure: check pairs with the WCAG ratio (`limits::TEXT_CONTRAST`) and, as advice, APCA. The token palette and `design_palette` do all of this for you.

### The 12-step ramp

Each hue is a ramp of 12 steps, light to dark in a light scheme (dark to light in dark), each step with one job. After Radix Colors:

| Step | Use |
|---|---|
| 1 | Window background |
| 2 | Subtle background: cards, sidebars, striped rows |
| 3 | Control background at rest |
| 4 | Control background on hover |
| 5 | Control background pressed or selected |
| 6 | Subtle borders and separators of things you cannot operate |
| 7 | Borders of controls |
| 8 | Strong borders, hovered borders, focus rings |
| 9 | Solid fill: the purest colour (a primary button, a badge) |
| 10 | Solid fill on hover |
| 11 | Secondary text and icons (APCA Lc 60 on step 2) |
| 12 | Main text (APCA Lc 90 on step 2) |

Text goes on steps 1 to 5 in steps 11 or 12; steps 9 and 10 take a foreground made for them (white or step 1, checked).

### Harmonies

Pick the brand hue H, keep L, and choose the others by angle in OKLCH:

- **Analogous** (H, H ± 30°): calm, for related categories.
- **Complementary** (H + 180°): the strongest contrast of hue; use it for the one accent that must be seen.
- **Split-complementary** (H + 150°, H + 210°): most of the energy of a complement with less tension; a good default for an accent pair.
- **Triadic** (H, H + 120°, H + 240°): for charts and tags that need distinct categories.

### Using a complement without vibration

Two complements at full chroma, side by side, seem to shimmer at their edge (simultaneous contrast). So: let them differ in **proportion** (the 60-30-10 weight: about 60 % tinted neutrals, 30 % the primary hue, 10 % the accent), in **chroma** (the large area muted, the small one vivid) and in **lightness**; and **never let them touch at full chroma** — put a neutral surface between an accent button and a strongly coloured panel.

### Tinted neutrals

Pure grey (C = 0) looks dead next to a brand colour. Give the neutral ramp the brand hue at a very low chroma (about 0.005 to 0.02) and the whole interface reads as one family. Keep text contrast; tinting barely moves L.

### Reserved semantic hues

Danger (red, H ≈ 25°), warning (amber, H ≈ 80°), success (green, H ≈ 150°) and info (blue, H ≈ 250°) keep their hues whatever the brand, so a brand accent is never mistaken for an error. A brand whose own hue falls in one of these bands takes its accent from a harmony instead. And never use the hue alone: give state a word or an icon too (`color-state-only`).

### Dark mode is not inverted light mode

Inverting turns shadows into glows and pure white into pure black. In dark mode: surfaces get lighter as they rise (a raised card is a step lighter, since shadows barely show on dark); avoid pure black and pure white text on it (lower chroma, L around 0.15 for the window, 0.93 for text); lower the chroma of large coloured areas, which glare on dark; check every pair again — a pair that passes in light can fail in dark.

### High contrast

When the system asks for more contrast (`Qt.styleHints.accessibility.contrastPreference`), text pairs aim at 7:1 (WCAG AAA), borders get stronger (step 8 instead of 6 or 7), translucency goes and subtle tints go to plain surfaces. Apple asks for at least a higher-contrast scheme when Increase Contrast is on if the default does not meet the minimums.

## Rules

<a id="color-text-contrast"></a>
### `color-text-contrast`

**Error, static and runtime.** Text does not stand out from what is behind it, in at least one colour scheme or contrast setting. Low-vision users, older users and anyone in sunlight cannot read it.

Numbers: `limits::TEXT_CONTRAST` (4.5:1), `limits::LARGE_TEXT_CONTRAST` (3:1) for text of `limits::LARGE_TEXT_PX` or more, or `limits::LARGE_BOLD_TEXT_PX` at `limits::BOLD_WEIGHT`. Not rounded: 4.49:1 fails. Disabled controls and logos are exempt. APCA advice beside it: `limits::APCA_BODY_LC` for body text, `limits::APCA_LARGE_LC` for large text.

```qml
// Bad: a tint of the surface as text
Rectangle { color: "#f4f4f4"; Label { text: qsTr("Last synced 5 min ago"); color: "#a0a0a0" } }
// Good: a pair made for each other
Rectangle { color: palette.base; Label { text: qsTr("Last synced 5 min ago"); color: palette.text } }
```

Sources: https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum.html , https://git.apcacontrast.com/documentation/APCA_in_a_Nutshell , https://developer.apple.com/design/human-interface-guidelines/accessibility

<a id="color-nontext-contrast"></a>
### `color-nontext-contrast`

**Warning, runtime.** A boundary, icon, focus ring or state indicator the user needs stands out less than `limits::NONTEXT_CONTRAST` (3:1) from what is next to it. A text field whose border cannot be seen is a field nobody finds. APCA advice: `limits::APCA_NONTEXT_LC`.

```qml
// Bad
TextField { background: Rectangle { border.color: "#e6e6e6" } }
// Good
TextField { background: Rectangle { color: palette.base; border.color: palette.mid } }
```

Sources: https://www.w3.org/WAI/WCAG22/Understanding/non-text-contrast.html

<a id="color-state-only"></a>
### `color-state-only`

**Warning, runtime.** A control's states (on and off, selected or not) differ only by hue or tint. About 1 in 12 men cannot tell red from green, and nobody can in grey scale.

```qml
// Bad: green for on, grey for off
Rectangle { color: online ? "seagreen" : "grey"; width: 10; height: 10; radius: 5 }
// Good: shape and words as well
Row {
    Rectangle { width: 10; height: 10; radius: 5; color: online ? "seagreen" : "transparent"; border.color: palette.windowText }
    Label { text: online ? qsTr("Online") : qsTr("Offline") }
}
```

Sources: https://www.w3.org/WAI/WCAG22/Understanding/use-of-color.html , https://developer.apple.com/design/human-interface-guidelines/accessibility

<a id="color-literal"></a>
### `color-literal`

**Hint, static.** A colour is a literal in a component instead of a palette role or a token, so it cannot follow dark mode or high contrast. Literals belong in one theme file.

```qml
// Bad
Label { color: "#333333" }
// Good
Label { color: palette.windowText }
```

Sources: https://m3.material.io/styles/color/roles , https://developer.apple.com/design/human-interface-guidelines/color

<a id="color-scheme-ignored"></a>
### `color-scheme-ignored`

**Warning, static.** A component paints its own surfaces with fixed colours and no binding to the palette or colour scheme, so one scheme is unreadable (white text on a white card in light mode, or the reverse).

```qml
// Bad
Rectangle { color: "white"; Label { text: qsTr("Notes"); color: palette.text } }
// Good
Rectangle {
    readonly property bool dark: Application.styleHints.colorScheme === Qt.ColorScheme.Dark
    color: dark ? "#1f2226" : "#ffffff"
    Label { text: qsTr("Notes"); color: palette.text }
}
```

Better: `color: palette.base`.

Sources: https://doc.qt.io/qt-6/qstylehints.html#colorScheme-prop , https://developer.apple.com/design/human-interface-guidelines/dark-mode

<a id="color-translucent-surface"></a>
### `color-translucent-surface`

**Warning, runtime.** A popup, menu or dialog lets what is behind show through enough to compete with its own text. Opacity at least `limits::SURFACE_MIN_ALPHA`, or a blur behind it.

```qml
// Bad
Menu { background: Rectangle { color: Qt.rgba(1, 1, 1, 0.7) } }
// Good
Menu { background: Rectangle { color: palette.window; border.color: palette.mid } }
```

Sources: https://developer.apple.com/design/human-interface-guidelines/materials , https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum.html

<a id="color-surface-merge"></a>
### `color-surface-merge`

**Warning, runtime.** A floating or adjacent surface is nearly the colour behind it and has no border or shadow, so its edge is invisible. The audit asks for an OKLab distance of `limits::SURFACE_MIN_DELTA_E` when there is neither.

```qml
// Bad: the popup is the window colour with nothing around it
Popup { background: Rectangle { color: palette.window } }
// Good: a step up and an edge
Popup { background: Rectangle { color: palette.base; border.color: palette.mid; radius: 6 } }
```

Sources: https://www.radix-ui.com/colors/docs/palette-composition/understanding-the-scale , https://www.refactoringui.com/

## Not checked yet

- **No complements touching at full chroma** (proposed `color-complement-adjacent`): see above.
- **Keep the semantic hues** (proposed `color-semantic-hue`): a brand accent inside the danger, warning, success or info band.
- **60-30-10** (proposed `color-accent-weight`): the accent covers about a tenth of the window, not a third.
- **Interactive states need more contrast than rest**, not less (Vercel). https://vercel.com/design/guidelines

## Sources

- WCAG 2.2, contrast and use of colour: https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum.html , https://www.w3.org/WAI/WCAG22/Understanding/non-text-contrast.html , https://www.w3.org/WAI/WCAG22/Understanding/use-of-color.html
- APCA in a Nutshell: https://git.apcacontrast.com/documentation/APCA_in_a_Nutshell
- Evil Martians, OKLCH in CSS: https://evilmartians.com/chronicles/oklch-in-css-why-quit-rgb-hsl
- Björn Ottosson, Oklab: https://bottosson.github.io/posts/oklab/
- Radix Colors, understanding the scale: https://www.radix-ui.com/colors/docs/palette-composition/understanding-the-scale
- Material 3 colour roles: https://m3.material.io/styles/color/roles
- Apple HIG colour and dark mode: https://developer.apple.com/design/human-interface-guidelines/color , https://developer.apple.com/design/human-interface-guidelines/dark-mode
- Refactoring UI: https://www.refactoringui.com/
