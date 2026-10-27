---
title: Colour and contrast
description: Choose palettes in OKLCH, support light, dark, and high-contrast schemes, and pass every text and non-text contrast check.
section: quality
order: 4
---

# Colour and contrast

This page explains how Ljós builds colour palettes in OKLCH, how to support light mode, dark mode, and high contrast using `palette` roles or `Ljos.Design` tokens, and what the `color-*` rules check in your application.

## Choosing colours in OKLCH

HSL lightness is a mathematical formula rather than a measure of human perception: yellow (`hsl(60 100% 50%)`) and blue (`hsl(240 100% 50%)`) share the same HSL lightness value but look completely different in brightness, so rotating hues in HSL silently breaks contrast.

Ljós works in **OKLCH**, where:

- **L** is perceived lightness (`0` to `1`).
- **C** is chroma (`0` to about `0.37`).
- **H** is the hue angle (`0°` to `360°`).

Two colours with the same OKLCH lightness read as equally light regardless of hue. When a high-chroma combination falls outside the sRGB gamut, chroma is reduced while keeping L and H fixed. Always verify pairs against the WCAG contrast ratio (`limits::TEXT_CONTRAST`) and APCA advice; the `Ljos.Design` token module and the `design_palette` and `design_contrast` MCP tools handle this automatically.

### The 12-step colour ramp

Each hue forms a 12-step ramp from light to dark in a light scheme (and dark to light in a dark scheme), where every step has a dedicated role:

| Step | Role |
|---|---|
| 1 | Window background |
| 2 | Subtle background: cards, sidebars, striped rows |
| 3 | Control background at rest |
| 4 | Control background on hover |
| 5 | Control background when pressed or selected |
| 6 | Subtle borders and separators on non-interactive elements |
| 7 | Borders of interactive controls |
| 8 | Strong borders, hovered borders, and focus rings |
| 9 | Solid fill: primary buttons and badges |
| 10 | Solid fill on hover |
| 11 | Secondary text and icons (APCA Lc 60 on step 2) |
| 12 | Main text (APCA Lc 90 on step 2) |

Place text in steps 11 or 12 over surfaces in steps 1 to 5. Solid fills in steps 9 and 10 take a dedicated high-contrast foreground (`textOnAccent`, such as `#ffffff` or step 1).

### Harmonies, 60-30-10 weight, and semantic hues

Starting from your brand hue `H` at a fixed lightness:

- **Analogous (`H`, `H ± 30°`):** Calm combinations for related categories.
- **Complementary (`H + 180°`):** Strongest hue contrast; reserve it for the single accent that must stand out.
- **Split-complementary (`H + 150°`, `H + 210°`):** Distinct accent pair with less visual tension than a direct complement.
- **Triadic (`H`, `H + 120°`, `H + 240°`):** Distinct categories for charts and tags.

Follow the **60-30-10 proportion**: about 60 % tinted neutral surfaces (neutral ramps given the brand hue at low chroma, `0.005` to `0.02`), 30 % primary structural hue, and at most 10 % accent colour. Never place two complementary hues side by side at full chroma, as their shared edge appears to shimmer; separate them with a neutral surface.

Keep semantic hues reserved regardless of brand colour:

- **Danger (red):** `H ≈ 25°`
- **Warning (amber):** `H ≈ 80°`
- **Success (green):** `H ≈ 150°`
- **Info (blue):** `H ≈ 250°`

### Dark mode and high contrast

Dark mode is not an inverted light mode:

- Surfaces become a step lighter as they rise in elevation, because drop shadows barely register on dark backgrounds.
- Avoid pure black surfaces and pure white text (aim around `L = 0.15` for the window background and `L = 0.93` for main text), and lower the chroma of large coloured areas so they do not glare.
- When the system requests high contrast (`Qt.styleHints.accessibility.contrastPreference`), text pairs aim for **7:1** (WCAG AAA), borders move to step 8, and translucent or subtly tinted surfaces become opaque and plain.

With `import Ljos.Design`, the `Colors` singleton adapts automatically to the system colour scheme and contrast setting:

```qml
import QtQuick
import Ljos.Design

Item {
    objectName: "root"
    width: 320
    height: 160

    property color canvas: Colors.canvas
    property color accent: Colors.accent
    property color textOnAccent: Colors.textOnAccent
    property color textMuted: Colors.textMuted
    property bool dark: Colors.dark
}
```

## Colour and contrast rules

### `color-text-contrast` (Error, static and runtime)

Text must stand out against its background across every colour scheme and contrast setting:

- Normal text requires a WCAG contrast ratio of at least **4.5:1** (`limits::TEXT_CONTRAST`), with APCA advice at `limits::APCA_BODY_LC`. Ratios are not rounded, so `4.49:1` fails.
- Large text (`limits::LARGE_TEXT_PX` at 24 px, or `limits::LARGE_BOLD_TEXT_PX` at 18.66 px bold with `limits::BOLD_WEIGHT`) requires at least **3:1** (`limits::LARGE_TEXT_CONTRAST`), with APCA advice at `limits::APCA_LARGE_LC`.
- Disabled controls and logos are exempt.

```qml
// Good: palette.base and palette.text are paired to pass contrast in every scheme
import QtQuick
import QtQuick.Controls

ApplicationWindow {
    width: 320
    height: 120
    visible: true
    title: qsTr("Sync status")

    Rectangle {
        anchors.fill: parent
        color: palette.base

        Label {
            anchors.centerIn: parent
            text: qsTr("Last synced 5 min ago")
            color: palette.text
        }
    }
}
```

### `color-nontext-contrast` (Warning, runtime)

Interactive boundaries, text field frames, checkbox and switch indicators, slider grooves and handles, focus rings, and icon-only button icons that your application draws must have at least **3:1** contrast (`limits::NONTEXT_CONTRAST`, APCA `limits::APCA_NONTEXT_LC`) against adjacent pixels.

```qml
import QtQuick
import QtQuick.Controls

ApplicationWindow {
    width: 320
    height: 120
    visible: true
    title: qsTr("Search")

    TextField {
        anchors.centerIn: parent
        width: 240
        Accessible.name: qsTr("Search query")
        background: Rectangle {
            color: palette.base
            border.color: palette.mid
            border.width: 1
            radius: 4
        }
    }
}
```

### `color-state-only` (Warning, runtime)

Never distinguish control states (such as online/offline or checked/unchecked) by hue or tint alone. Pair colour changes with a label, icon, or shape change so users with colour-vision deficiency or greyscale displays can tell the states apart by at least 3:1:

```qml
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ApplicationWindow {
    width: 320
    height: 120
    visible: true
    title: qsTr("Connection")

    property bool online: true

    RowLayout {
        anchors.centerIn: parent
        spacing: 8

        Rectangle {
            width: 10
            height: 10
            radius: 5
            color: online ? "seagreen" : "transparent"
            border.color: palette.windowText
            border.width: 2
        }

        Label {
            text: online ? qsTr("Online") : qsTr("Offline")
        }
    }
}
```

### `color-literal` (Hint, static) and `color-scheme-ignored` (Warning, static)

- **`color-literal`:** Avoid hard-coding colour literals (`"#333333"`) inside components; take colours from `palette` roles or `Ljos.Design` tokens so they adapt to dark mode and high contrast. Keep literal definitions in a single theme file.
- **`color-scheme-ignored`:** Never paint a component surface with a fixed opaque colour while binding text to `palette.text`, or one colour scheme will become unreadable. Bind the surface to `palette.base` or `palette.window`, or branch on `Application.styleHints.colorScheme === Qt.ColorScheme.Dark`.

### `color-translucent-surface` (Warning, runtime) and `color-surface-merge` (Warning, runtime)

- **`color-translucent-surface`:** Popups, menus, and dialogs must be opaque (alpha at least `limits::SURFACE_MIN_ALPHA`, `0.95`) or blurred so background content does not bleed through and compete with the popup's text.
- **`color-surface-merge`:** An open popup must stand apart from the surface behind it through a colour step (at least `limits::SURFACE_MIN_DELTA_E` in OKLab), a border, or a shadow.

```qml
import QtQuick
import QtQuick.Controls

ApplicationWindow {
    width: 320
    height: 200
    visible: true
    title: qsTr("Details")

    Popup {
        anchors.centerIn: parent
        width: Math.min(implicitWidth, 280)
        visible: true
        focus: true
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside

        background: Rectangle {
            color: palette.base
            border.color: palette.mid
            border.width: 1
            radius: 6
        }

        contentItem: Label {
            text: qsTr("Popup surface separated by a step and border.")
            wrapMode: Text.Wrap
        }
    }
}
```

### `color-semantic-hue` (Hint, runtime) and `color-accent-weight` (Hint, runtime)

- **`color-semantic-hue`:** Do not paint destructive controls or error states green (`limits::hue::GREEN`) or confirmation controls red (`limits::hue::RED`). Use red for destructive actions and green for confirmations.
- **`color-accent-weight`:** Keep the accent colour under `limits::hue::ACCENT_MAX_SHARE` (about 10 % of the window) and highlight at most one primary button (`limits::hue::PRIMARY_BUTTONS_MAX`) per surface:

```qml
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ApplicationWindow {
    width: 360
    height: 120
    visible: true
    title: qsTr("Publish post")

    RowLayout {
        anchors.centerIn: parent
        spacing: 8

        Button {
            text: qsTr("Delete draft")
            palette.button: "#c01c28"
            palette.buttonText: "white"
        }

        Button {
            text: qsTr("Save draft")
        }

        Button {
            text: qsTr("Publish post")
            highlighted: true
        }
    }
}
```

## See also

- [Design guardrails](design-guardrails.md)
- [Accessibility](accessibility.md)
- [Motion and reduced motion](motion.md)
- [Layout, content, and typography](layout-and-typography.md)
- [First app](../start/first-app.md)
