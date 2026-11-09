// Ljós's own style (docs/DIALECT.md §4.16): the roles every control of
// Ljos.Controls draws with, derived as crates/ljos_design/src/roles.rs
// derives them (a test holds the two to the same colours).
pragma Singleton
import QtQuick
import Ljos.Design

// Three colours give every other role. Until an app sets them they are
// Ljos.Design's, so the style is light or dark as the platform is and a
// theme.json restyles it. Each role is a plain property: an app may set
// any one of them, and setting `canvas`, `text` or `accent` derives the
// rest again.
QtObject {
    id: theme

    property color canvas: Colors.canvas
    property color text: Colors.text
    property color accent: Colors.accent

    // Lifted toward the text: a little for a pane, more for a field or a
    // button, most for a menu.
    property color surface: mix(canvas, text, 0.04)
    property color surfaceRaised: mix(canvas, text, 0.07)
    property color surfaceOverlay: mix(canvas, text, 0.10)
    property color border: mix(canvas, text, 0.16)
    property color accentSurface: mix(surfaceOverlay, accent, 0.18)

    // What is read, made legible on the fills it is drawn on. A highlighted
    // row is a fill too; a label or a pane is only ever on a ground.
    readonly property var fills: [canvas, surface, surfaceRaised, surfaceOverlay, accentSurface]
    readonly property var grounds: [canvas, surface, surfaceRaised, surfaceOverlay]
    property color input: reach(border, text, grounds, 3)
    property color textMuted: reach(mix(canvas, text, 0.62), text, fills, 4.5)
    property color placeholder: reach(mix(canvas, text, 0.5), text, [surfaceRaised], 4.5)
    property color iconMuted: reach(mix(canvas, text, 0.45), text, fills, 3)
    property color accentForeground: on(accent)
    property color focus: reach(accent, text, fills, 3)
    property color link: reach(accent, text, grounds, 4.5)
    property color errorForeground: reach(Qt.rgba(229 / 255, 72 / 255, 77 / 255, 1), text, fills, 4.5)
    property color warningForeground: reach(Qt.rgba(217 / 255, 119 / 255, 6 / 255, 1), text, fills, 4.5)

    // A filled button's fill in each state, moved away from its ink so the
    // ink only gains contrast as the button is hovered and pressed.
    property color accentHover: shade(accent, accentForeground, 0.08)
    property color accentPressed: shade(accent, accentForeground, 0.16)
    property color destructive: errorForeground
    property color destructiveForeground: on(destructive)
    property color destructiveHover: shade(destructive, destructiveForeground, 0.08)
    property color destructivePressed: shade(destructive, destructiveForeground, 0.16)

    readonly property bool dark: luminance(canvas) < luminance(text)

    // Sizes: a compact desktop scale, every target at least 24 pixels.
    property real radius: 8
    readonly property real radiusSmall: Math.round(radius * 0.5)
    readonly property real radiusLarge: Math.round(radius * 1.5)
    property int controlHeight: 28
    readonly property int spacingSmall: 4
    readonly property int spacing: 8
    property int fontSize: 13
    property int fontSizeSmall: 12
    property int iconSize: 16
    readonly property int iconSizeSmall: 12
    // Thinner strokes as an icon grows, so it reads as one weight.
    function stroke(size) {
        return size <= 12 ? 2 : size <= 16 ? 1.75 : 1.5
    }

    // `a` moved `t` of the way to `b`, rounded to 8 bits a channel.
    function mix(a: color, b: color, t: real): color {
        function ch(x, y) {
            return Math.max(0, Math.min(255, Math.round((x + (y - x) * t) * 255))) / 255
        }
        return Qt.rgba(ch(a.r, b.r), ch(a.g, b.g), ch(a.b, b.b), 1)
    }

    // WCAG 2.2's relative luminance and contrast ratio.
    function luminance(c: color): real {
        function lin(x) {
            return x <= 0.04045 ? x / 12.92 : Math.pow((x + 0.055) / 1.055, 2.4)
        }
        return 0.2126 * lin(c.r) + 0.7152 * lin(c.g) + 0.0722 * lin(c.b)
    }
    function contrast(a: color, b: color): real {
        const la = luminance(a)
        const lb = luminance(b)
        return (Math.max(la, lb) + 0.05) / (Math.min(la, lb) + 0.05)
    }

    // The first colour from `from` toward `to`, in 40 steps, with `ratio`
    // against every colour of `against` (one colour or a list); `to` when
    // none has it.
    function reach(from: color, to: color, against: var, ratio: real): color {
        const list = Array.isArray(against) ? against : [against]
        for (let i = 0; i <= 40; i++) {
            const c = mix(from, to, i / 40)
            let least = Infinity
            for (let j = 0; j < list.length; j++)
                least = Math.min(least, contrast(c, list[j]))
            if (least >= ratio)
                return c
        }
        return to
    }

    // `fill` moved `t` of the way away from `ink`.
    function shade(fill: color, ink: color, t: real): color {
        return mix(fill, on(ink), t)
    }

    // Black or white, whichever has more contrast with `c`.
    function on(c: color): color {
        const white = Qt.rgba(1, 1, 1, 1)
        const black = Qt.rgba(0, 0, 0, 1)
        return contrast(white, c) >= contrast(black, c) ? white : black
    }
}
