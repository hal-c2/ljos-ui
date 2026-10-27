---
title: Design guardrails
description: Check your QML application for accessibility, colour, layout, typography, focus, motion, and content problems with ljos check and ljos audit.
section: quality
order: 2
---

# Design guardrails

This page shows you how to check your QML code for design and accessibility defects using `ljos check` and `ljos audit`, how to use the `Ljos.Design` token module so most issues never arise, and how to fix or waive a finding.

## Four layers of checks

Prose guidelines are easy to overlook, so Ljós turns its design rules into executable checks across four layers that all read from the same rule table (`ljos_design::RULES`):

1. **Design tokens (`Ljos.Design`):** A built-in QML module gives you colour roles grown from a single seed in OKLCH, motion durations and easing curves by frequency, a 4 px spacing scale, and a type scale. Because `ljos design export <dir>` writes the same module out as plain QML files, an application that also runs on Qt can use the exact same tokens.
2. **Static lints (`ljos check` and `qml_lint`):** Static analysis inspects your QML source files for mistakes that are visible without running the app, such as ease-in on an entrance animation, scaling from zero, animating `width` or `height`, setting anchors on a direct child of a layout, dynamic text in a bounded box without `elide` or `wrapMode`, pointer-only actions, literal colours in components, or unnamed interactive items.
3. **Runtime audit (`ljos audit` and `app_audit`):** The runtime audit lays out and draws the live scene across a test matrix—light and dark schemes, normal and high contrast, font scales up to 200 %, and narrow and short window sizes—and measures what was actually painted. It catches low contrast against composited backgrounds, clipped text, controls pushed outside the window or covered by overlays, missing keyboard focus rings, and popups that do not fit.
4. **MCP design tools (`ljos mcp`):** Authoring tools (`design_palette`, `design_contrast`, `design_motion`, and `design_rules`) compute passing values before you write code.

## Using design tokens

On Ljós, `import Ljos.Design` is built in and requires no extra files in your project. Its colours follow the system colour scheme and contrast preference automatically:

```qml
import QtQuick
import QtQuick.Controls
import Ljos.Design

ApplicationWindow {
    objectName: "root"
    width: 400
    height: 240
    visible: true
    title: qsTr("Notes")
    color: Colors.canvas

    Pane {
        anchors.centerIn: parent
        Label {
            text: qsTr("Synced with workspace")
            color: Colors.textMuted
        }
    }
}
```

If your application also runs on Qt (`qml6`), export the token module as plain QML files into your repository:

```sh
ljos design export qml/
```

## Running static checks and runtime audits

Use the tools in order as you write and test a component:

```sh
# 1. Compile and run static design lints on a file or directory
ljos check app/Main.qml

# 2. Downgrade errors to warnings when inspecting third-party code
ljos check --lint=warn corpus/

# 3. Compile without running design lints
ljos check --no-lint app/Main.qml

# 4. Run the headless runtime audit over the full matrix (light/dark, high contrast, 200 % font scale, small window)
ljos audit app/Main.qml

# 5. Run a quick headless audit at the current window setting only
ljos audit app/Main.qml --matrix current

# 6. Output audit findings as JSON or filter by rule areas/codes
ljos audit app/Main.qml --matrix current --rules color,focus,motion --json
```

When your application requires a running session or user interaction to reach a specific state, run it with the MCP inspector enabled or open the visual observer:

```sh
# Start the application with the MCP inspector server enabled
ljos run --mcp app/Main.qml

# Or open the visual observer and use the Audit page to highlight findings on the window
ljos observe
```

From an MCP client connected to `ljos run --mcp`, call `app_audit` with `{"matrix": "full"}` (or `{"matrix": "current"}`, the default). The audit tests the live scene and restores the window state when it finishes. In Gherkin feature files, use the step `the design audit finds nothing` to assert that a scenario passes all checks.

## Severity levels and waiving findings

Severity reflects how certain the check is that a user is hurt:

| Severity | Meaning | Effect |
|---|---|---|
| **Error** | Certainly wrong for some user. | Fails `ljos check` and `ljos audit` with a non-zero exit code. |
| **Warning** | Very likely wrong. | Reported; must be fixed or kept with a written reason. |
| **Hint** | Advice that is sometimes right to ignore. | Reported only. |

If you intentionally keep code that triggers a Warning, add a `// ljos-allow(<code>): <reason>` comment on the same line, the line above, or the first line of the enclosing QML object:

```qml
import QtQuick

Item {
    width: 200
    height: 100

    NumberAnimation {
        // ljos-allow(motion-duration): the onboarding tour is watched once, on purpose
        duration: 900
        easing.type: Easing.OutCubic
    }
}
```

Two rules keep allow comments honest:

- **`lint-allow-without-reason` (Error):** Writing `// ljos-allow(motion-duration)` without a reason after the colon fails `ljos check`, because the next reader needs to know why the rule does not apply.
- **`lint-allow-unused` (Hint):** When a rule no longer triggers at that location, `ljos check` reports the stale allow comment so you can delete it.

## Example finding and how to fix it

Every finding reports the rule code, the item's accessible role, name, and locator, the source file and line, what was measured against the limit (for example, a contrast ratio of `2.9:1 < 4.5:1`), the matrix point where it failed, a suggested fix, and a link to the rule guide. <!-- check: exact CLI text formatting of a printed finding from ljos check / ljos audit -->

Consider a component with three common issues: a literal grey text colour that fails contrast (`color-text-contrast` and `color-literal`), an icon-only button without an accessible name or tooltip (`a11y-unnamed-icon-button` and `a11y-icon-button-no-tooltip`), and a dynamic label inside a bounded row with no elide mode (`text-unbounded-dynamic`):

```qml
// Bad: triggers color-text-contrast, color-literal, a11y-unnamed-icon-button,
// a11y-icon-button-no-tooltip, and text-unbounded-dynamic
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ApplicationWindow {
    width: 360
    height: 120
    visible: true
    title: qsTr("Sync status")

    property string folderName: "Quarterly financial statements"

    Rectangle {
        anchors.fill: parent
        color: "#f4f4f4"

        RowLayout {
            anchors.fill: parent
            anchors.margins: 12
            spacing: 8

            Label {
                Layout.preferredWidth: 120
                text: folderName
                color: "#a0a0a0"
            }

            ToolButton {
                icon.name: "view-refresh"
            }
        }
    }
}
```

To fix these findings, bind surfaces and text to `palette` roles (or `Ljos.Design` tokens), set `elide` and a conditional `ToolTip` on the bounded label (`text-elided-hidden`), and give the icon-only button `text`, `display: AbstractButton.IconOnly`, and a `ToolTip` that appears on hover and keyboard focus:

```qml
// Good: passes both ljos check and ljos audit across the full matrix
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ApplicationWindow {
    width: 360
    height: 120
    visible: true
    title: qsTr("Sync status")

    property string folderName: "Quarterly financial statements"

    Pane {
        anchors.fill: parent
        padding: 12

        RowLayout {
            anchors.fill: parent
            spacing: 8

            Label {
                id: folderLabel
                Layout.fillWidth: true
                text: folderName
                color: palette.text
                elide: Text.ElideRight

                HoverHandler {
                    id: labelHover
                }

                ToolTip.visible: folderLabel.truncated && labelHover.hovered
                ToolTip.text: folderName
            }

            ToolButton {
                text: qsTr("Refresh folder")
                display: AbstractButton.IconOnly
                icon.name: "view-refresh"
                ToolTip.text: text
                ToolTip.visible: hovered || visualFocus
                ToolTip.delay: 500
            }
        }
    }
}
```

## Pre-flight checklist

Before you write a new component, answer eight questions:

1. **Frequency of use:** Actions used many times a day (list selection, tab switch, shortcut) should be instant or take at most `limits::motion::FREQUENT_MAX_MS` (100 ms). Rare panels or dialogs can take up to `limits::motion::STANDARD_MAX_MS` (350 ms).
2. **Purpose of motion:** Only animate to confirm input, show where an item came from or went, indicate a state change, or prevent a jarring jump.
3. **Colour weight (60-30-10):** Use about 60 % tinted neutral surfaces, 30 % primary structural hue, and at most 10 % accent colour for the single primary call to action.
4. **All control states:** Account for rest, hover, pressed, keyboard focus (`visualFocus`), disabled (with a visible reason), checked/selected, loading, empty, and error states without relying on hue alone.
5. **Keyboard path:** Ensure Tab reaches every operable control in reading order, Enter or Space activates it, arrow keys move within composite groups, and Escape closes popups.
6. **Accessible names:** Start every control's accessible name with its visible text, and give icon-only buttons `text`, `display: AbstractButton.IconOnly`, and a tooltip.
7. **Small windows and 200 % text:** Decide beforehand what wraps, what elides (with a tooltip), and what scrolls so no control leaves the window.
8. **Dark mode and high contrast:** Take every colour from a `palette` role or `Ljos.Design` token rather than hex literals.

## See also

- [Goals and principles](principles.md)
- [Accessibility](accessibility.md)
- [Colour and contrast](color.md)
- [Motion and reduced motion](motion.md)
- [Layout, content, and typography](layout-and-typography.md)
- [First app](../start/first-app.md)
