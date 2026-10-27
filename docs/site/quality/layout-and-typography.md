---
title: Layout, content, and typography
description: Build QML layouts that hold at every window size and up to 200 % font scale, keep text readable, and write clear interface copy.
section: quality
order: 6
---

# Layout, content, and typography

This page explains how to build QML layouts that adapt to narrow or short windows and 200 % text scaling, how to size and wrap typography without clipping, and how to write clear interface labels and empty states.

## Layout principles and rules

A layout works only when it holds at every window size and font scale a user chooses. The runtime audit (`ljos audit`) lays out your scene at the default window size, at narrow and short fractions (`limits::audit::NARROW_FRACTION` and `limits::audit::SHORT_FRACTION`), and across font scales (`limits::audit::FONT_SCALES`, up to 200 %), checking whether any control overflows, overlaps, or gets covered.

### Core layout principles

- **Use `QtQuick.Layouts` (`RowLayout`, `ColumnLayout`, `GridLayout`):** Layouts respect each child's `implicitWidth`, `implicitHeight`, and minimum sizes as text grows. Fixed `x`/`y` coordinates and manual anchors break when strings are translated or scaled.
- **Size surfaces from content:** Let popups, cards, and dialogs take their size from `implicitWidth` and `implicitHeight` up to a maximum, scrolling when content exceeds the available space.
- **Follow the 4 px spacing scale (`layout-spacing-scale`):** Keep every `spacing`, `margins`, and `padding` value a multiple of `limits::SPACING_UNIT_PX` (`4`, `8`, `12`, `16`, `24`, `32`).
- **Match nested corner radii (`layout-nested-radius`):** When a `Rectangle` fills another with a margin (`anchors.margins: 2`), make the inner `radius` equal to the outer `radius` minus the gap, never larger.

### Layout rules

| Rule code | Severity | What it checks and how to fix it |
|---|---|---|
| `layout-offscreen` | Error (runtime) | An operable control lies partly or wholly outside the window at some window size or font scale with no `ScrollView` to reach it. Put long content inside a `ScrollView` and keep action buttons pinned in a `ColumnLayout`. |
| `layout-occluded` | Error (runtime) | An operable control is covered by another item drawn above it, such as a banner or toast anchored over a footer button. Give the notice its own row in the layout. |
| `layout-anchors-in-layout` | Error (static) | A direct child of a `RowLayout`, `ColumnLayout`, or `GridLayout` sets `anchors.*`. Remove the anchors and use `Layout.fillWidth`, `Layout.alignment`, or a spacer `Item` instead. |
| `layout-overlap` | Warning (runtime) | Two sibling labels or controls cross each other by more than 1 px because anchors ignore growing text. Place them in a `RowLayout` with `Layout.fillWidth: true` and `elide: Text.ElideRight`. |
| `layout-under-minimum` | Warning (runtime) | An item is laid out smaller than its `Layout.minimumWidth` or `implicitWidth`. Set the window's `minimumWidth` and `minimumHeight` to at least the sum of its panes' minimums. |
| `layout-popup-fit` | Warning (runtime) | A popup or dialog is much larger than its content (filling less than `limits::POPUP_MIN_FILL`) or too small to show it. Omit fixed dimensions so the dialog sizes to its content. |
| `layout-fixed-size` | Warning (static) | A popup, dialog, or pane with dynamic content sets a literal `width` and `height`. Bind `width: Math.min(implicitWidth, 400)` and wrap the text. |
| `layout-shift` | Warning (runtime) | Content moves by at least `limits::LAYOUT_SHIFT_MIN_PX` after settling (within 500 ms) without user input (`limits::LAYOUT_SHIFT_INPUT_GRACE_MS`). Reserve space for status or error lines using `opacity: error !== "" ? 1 : 0` instead of toggling `visible`. |
| `layout-stacked-scrims` | Warning (runtime) | Two modal overlays dim the window (`dim: true`) at the same time. Close the first drawer or popup before opening the second, or dim only the topmost layer. |
| `layout-resize-handle` | Warning (runtime) | A `SplitView` handle draws nothing at rest. Give the handle a visible `Rectangle` that changes colour on `SplitHandle.hovered` and `SplitHandle.pressed`. |
| `layout-window-controls` | Warning (runtime) | A frameless window (`Qt.FramelessWindowHint`) lacks a visible close button at the top corner within `limits::WINDOW_CONTROLS_MAX_INSET_PX`. Add a `ToolButton` named `Close` in the header `ToolBar`. |
| `layout-reflow` | Warning (runtime) | At `limits::REFLOW_WIDTH_PX` (`320 px` wide), a `ScrollView` or `Flickable` containing reading text scrolls both horizontally and vertically. Bind the label's `width` to `view.availableWidth` with `wrapMode: Text.Wrap`. |
| `layout-misaligned` | Hint (runtime) | Stacked controls in a column have left edges differing by 1 to `limits::MISALIGN_MAX_PX`, or form fields of the same kind have inconsistent widths. Align them in a `GridLayout` with `Layout.fillWidth: true`. |
| `layout-spacing-scale` | Hint (static) | A `spacing`, `margins`, or `padding` literal is not a multiple of 4 px (for example, `7` or `13`). Round to the 4 px grid (`8` or `12`). |
| `layout-wrap-separator` | Hint (runtime) | A wrapped `Flow` starts or ends a line with a standalone separator (`·`, `|`, `ToolSeparator`). Attach the separator prefix to each item after index 0 instead. |
| `layout-menu-ungrouped` | Hint (static, runtime) | A `Menu` has more than `limits::MENU_MAX_UNGROUPED` (`10`) items without a `MenuSeparator` or submenu. Group related actions with `MenuSeparator {}`. |
| `layout-nested-radius` | Hint (static) | An inner `Rectangle` filling a parent `Rectangle` has a larger literal `radius` than the outer one. Set the inner radius to the outer radius minus the margin. |

```qml
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ApplicationWindow {
    width: 480
    height: 320
    minimumWidth: 320
    minimumHeight: 240
    visible: true
    title: qsTr("Preferences")

    property string errorMessage: ""

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 16
        spacing: 12

        ScrollView {
            id: formScroll
            Layout.fillWidth: true
            Layout.fillHeight: true

            GridLayout {
                width: formScroll.availableWidth
                columns: 2
                columnSpacing: 12
                rowSpacing: 8

                Label {
                    text: qsTr("Display name")
                }
                TextField {
                    Layout.fillWidth: true
                    Accessible.name: qsTr("Display name")
                }

                Label {
                    text: qsTr("Email address")
                }
                TextField {
                    Layout.fillWidth: true
                    Accessible.name: qsTr("Email address")
                }
            }
        }

        Label {
            Layout.fillWidth: true
            text: errorMessage
            color: palette.BrightText
            opacity: errorMessage !== "" ? 1 : 0
            wrapMode: Text.Wrap
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: 8

            Item {
                Layout.fillWidth: true
            }

            Button {
                text: qsTr("Save preferences")
                highlighted: true
            }
        }
    }
}
```

## Typography scale and rules

Use a small number of text sizes and establish hierarchy with font weight and colour before adding more sizes. Material 3's scale is a solid baseline:

| Role | Size / line height (px) | Typical use |
|---|---|---|
| Headline small | `24 / 32` | Page titles |
| Title large | `22 / 28` | Dialog titles |
| Body large | `16 / 24` | Long-form reading text |
| Body medium | `14 / 20` | Most interface controls and labels |
| Body small | `12 / 16` | Secondary lines and captions |
| Label small | `11 / 16` | Smallest badge or caption |

Never draw text smaller than `limits::TEXT_MIN_PX` (`10 px`). Keep paragraph line height around `1.5×` the font size and line length between 45 and 75 characters (never above `limits::LINE_LENGTH_MAX_CHARS`, `80` characters).

### Typography rules

- **`text-clipped` (Error, runtime):** Text is cut off by its container, a sibling, or the window edge at some window size or font scale up to 200 %. Let containers grow with `implicitHeight` and set `wrapMode: Text.Wrap` or `elide: Text.ElideRight`.
- **`text-unbounded-dynamic` (Warning, static) and `text-elided-hidden` (Warning, runtime):** Any dynamic binding (`text: model.title` or `qsTr(...)`) inside a width-bounded box must set `elide` or `wrapMode`. When text elides (`elide: Text.ElideRight`), expose the full string in a `ToolTip` while `truncated` is true.
- **`text-raw-markup` (Warning, runtime):** Plain text showing raw Markdown (`**bold**`, backticks) or HTML tags (`<b>`) looks broken. Set `textFormat: Text.MarkdownText` or strip the markup.
- **`text-too-small` (Warning, static and runtime):** Text below `10 px` (`limits::TEXT_MIN_PX`) is unreadable. To make a caption secondary, keep `font.pixelSize: 12` and mute its colour with `palette.placeholderText` while still passing `color-text-contrast`.
- **`text-font-literal` (Hint, static):** Leave `font.family` unset on `Label` and `Text` items so they inherit the application and platform font.
- **`text-line-length` (Hint, runtime):** Bound multi-line reading text (`limits::LINE_LENGTH_MIN_LINES` or more lines) to `width: Math.min(parent.width, 36 * font.pixelSize)` so lines stay under 80 characters.

```qml
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ApplicationWindow {
    width: 420
    height: 200
    visible: true
    title: qsTr("Document summary")

    property string docTitle: "Q4 Architecture Review and Migration Plan"
    property string helpText: "Review each section below before exporting the final bundle for your target devices."

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 16
        spacing: 8

        Label {
            id: heading
            Layout.preferredWidth: 240
            text: docTitle
            elide: Text.ElideRight

            HoverHandler {
                id: headingHover
            }

            ToolTip.visible: heading.truncated && headingHover.hovered
            ToolTip.text: docTitle
        }

        Label {
            width: Math.min(parent.width, 36 * font.pixelSize)
            text: helpText
            wrapMode: Text.Wrap
        }

        Label {
            textFormat: Text.MarkdownText
            text: qsTr("Saved to `notes.md`")
            font.pixelSize: 12
        }
    }
}
```

## Content rules and copy guidelines

Words are the first part of an interface a user reads.

### Checked content rules

- **`content-raw-identifier` (Warning, runtime):** Never display raw UUIDs, commit hashes (`limits::HASH_MIN_CHARS` or more hex characters), absolute file paths, URLs with query strings, or `QQuickItem(0x…)` object dumps as primary label text. Show a human-readable title and put the identifier in a `ToolTip` if needed.
- **`content-empty-state` (Warning, runtime):** Any `ListView` or `GridView` larger than `limits::EMPTY_VIEW_MIN_PX` with zero rows must show either a running `BusyIndicator` while loading or a placeholder `Label` explaining why the view is empty and what to do next.
- **`content-duplicate-control` (Warning, static):** Do not place two interactive controls in the same view that write to the same property.
- **`content-inconsistent-words` (Hint, runtime):** Use one term for an action across the window (`Retry` everywhere, not `Retry` in one button and `Try again` in a menu; checked against `ljos_design::words::SYNONYMS`).
- **`content-repeated-text` (Hint, runtime):** Do not repeat the heading text in the subtitle right below it (such as `Downloads` followed by `Your downloads`); use the second line for counts or status.
- **`content-identical-rows` (Hint, runtime):** Do not give `limits::IDENTICAL_ROWS_MIN` or more rows in a list the exact same label.
- **`content-disabled-no-reason` (Hint, runtime):** When disabling a control (`enabled: form.valid`), explain why in a label beside it (`Add a title to publish`) or in `Accessible.description` / `ToolTip.text`.
- **`content-vague-action` (Hint, static):** Label buttons with a verb and object (`Save draft`, `Delete 3 files`), never `OK`, `Yes`, `No`, `Continue`, `Submit`, `Done`, `Go`, `Click here`, or `Proceed`.
- **`content-menu-ellipsis` (Hint, static):** End a `MenuItem` or `Action` label with `…` (`Rename…`) when its handler opens a `Dialog`, `FileDialog`, or `Popup` for further input.

```qml
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ApplicationWindow {
    width: 400
    height: 280
    visible: true
    title: qsTr("Search results")

    property bool loading: false
    property string query: "invoice"
    property bool canPublish: false

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 16
        spacing: 12

        ListView {
            id: resultsView
            Layout.fillWidth: true
            Layout.fillHeight: true
            model: 0

            BusyIndicator {
                anchors.centerIn: parent
                running: loading
            }

            Label {
                anchors.centerIn: parent
                visible: !loading && resultsView.count === 0
                text: qsTr("No results for “%1”. Try a shorter search.").arg(query)
            }
        }

        RowLayout {
            spacing: 8

            Button {
                text: qsTr("Publish report")
                enabled: canPublish
            }

            Label {
                visible: !canPublish
                text: qsTr("Select at least one result to publish")
                color: palette.placeholderText
            }
        }
    }
}
```

### Content review guidelines

Check these five points by hand when reviewing an interface:

1. **Write informative toasts:** State what happened, how many items were affected, and provide an `Undo` action (`Moved 3 conversations to the archive`). Keep the toast visible while hovered or focused.
2. **Title settings pages consistently:** Use either all nouns (`Appearance`, `Notifications`, `Accounts`) or all verbs across settings pages, matching the navigation item that opened each page.
3. **Show row state on the row:** Indicate whether an item is pinned, muted, or archived directly on the row rather than hiding its state inside a context menu.
4. **Describe only what the build includes:** Never mention browser sync or online accounts in an offline or desktop-only build.
5. **Avoid ambiguous one-word statuses:** Write `Sync paused by you` or `No backup yet` rather than `Sync — Paused` or `Backups — None`.

## See also

- [Design guardrails](design-guardrails.md)
- [Accessibility](accessibility.md)
- [Colour and contrast](color.md)
- [Motion and reduced motion](motion.md)
- [First app](../start/first-app.md)
