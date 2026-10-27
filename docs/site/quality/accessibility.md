---
title: Accessibility
description: Build QML applications that work with screen readers, voice control, switches, and keyboards by giving controls clear names, roles, focus indicators, and contrast.
section: quality
order: 3
---

# Accessibility

This page explains how to make your Ljós application work with screen readers, voice control, switches, and keyboards by setting accessible names and roles, managing keyboard focus and Tab order, sizing pointer targets, and maintaining contrast.

## How the accessibility tree works in QML

Assistive technologies and Ljós's own test and MCP tools interact with your interface through its AccessKit accessibility tree and its Tab walk rather than its pixels:

- **Controls** (`Button`, `CheckBox`, `ItemDelegate`, and other `QtQuick.Controls` types) set their role, keyboard handling, and accessible name automatically from their `text` property.
- **Custom items** (`Item` or `Rectangle`) need `Accessible.role` and `Accessible.name` explicitly when they act as controls.
- **Decorative elements** should set `Accessible.ignored: true` so screen readers skip them.

## Accessible names and roles

Every interactive element, meaningful image, dialog, and input field needs an unambiguous name.

### Naming controls and icon buttons

- **`a11y-unnamed-interactive` (Error, static and runtime):** Any item that reacts to a click, tap, or key press must have an accessible name. Prefer a `Button` over a bare `Rectangle` with a `TapHandler`. If you must build a custom item, set `Accessible.role`, `Accessible.name`, and `Accessible.onPressAction`.
- **`a11y-unnamed-icon-button` (Error, static and runtime) and `a11y-icon-button-no-tooltip` (Warning, static and runtime):** For an icon-only button, set `text` to its accessible name, hide the label visually with `display: AbstractButton.IconOnly`, and show a `ToolTip` on both hover and `visualFocus`.
- **`a11y-name-mismatch` (Warning, static and runtime):** When you override `Accessible.name` on a control that shows visible text, start the accessible name with the visible words in the same order so voice-control users can say what they see (for example, `text: qsTr("Send")` with `Accessible.name: qsTr("Send message")`).

```qml
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ApplicationWindow {
    width: 360
    height: 120
    visible: true
    title: qsTr("Toolbar example")

    RowLayout {
        anchors.centerIn: parent
        spacing: 8

        ToolButton {
            text: qsTr("Delete selected rows")
            display: AbstractButton.IconOnly
            icon.name: "edit-delete"
            ToolTip.text: text
            ToolTip.visible: hovered || visualFocus
            ToolTip.delay: 500
        }

        Button {
            text: qsTr("Send")
            Accessible.name: qsTr("Send message")
        }
    }
}
```

### Avoiding duplicate or ambiguous names

- **`a11y-duplicate-name` (Warning, runtime):** Never give two actionable sibling controls the same name (such as two adjacent `Retry` buttons). Name each after what it retries (`Retry upload` and `Retry download`).
- **`a11y-ambiguous-name` (Warning, runtime):** When a button appears on every row of a list or table, include the row's subject in its accessible name so screen reader users browsing a list of controls can tell them apart:

```qml
import QtQuick
import QtQuick.Controls

ListView {
    width: 320
    height: 240
    model: ListModel {
        ListElement { title: "Invoice_Q1.pdf" }
        ListElement { title: "Invoice_Q2.pdf" }
    }

    delegate: ItemDelegate {
        required property string title
        width: ListView.view.width
        text: title

        ToolButton {
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            text: qsTr("More options for %1").arg(title)
            display: AbstractButton.IconOnly
            icon.name: "overflow-menu"
            ToolTip.text: text
            ToolTip.visible: hovered || visualFocus
        }
    }
}
```

### Images, text fields, dialogs, and live status

- **`a11y-unnamed-image` (Warning, static and runtime):** Give informative images an `Accessible.name` and mark decorative images with `Accessible.ignored: true`.
- **`a11y-placeholder-label` (Warning, static):** Never rely on `placeholderText` alone to label a `TextField` or `TextArea`. Placeholders vanish as soon as the user types and are drawn at low contrast. Pair the field with a `Label` or set `Accessible.name`.
- **`a11y-unnamed-dialog` (Warning, static and runtime):** Give every `Dialog` or modal `Popup` a `title` or `Accessible.name`.
- **`a11y-live-status` (Hint, runtime):** Mark status lines or toasts with `Accessible.role: Accessible.StatusBar` (or `Accessible.AlertMessage` for urgent errors) so screen readers announce updates when the text changes.

```qml
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ApplicationWindow {
    width: 360
    height: 240
    visible: true
    title: qsTr("Account settings")

    property int savedCount: 3

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 16
        spacing: 12

        RowLayout {
            spacing: 8
            Image {
                source: "status-ok.svg"
                Accessible.name: qsTr("Connected")
            }
            Image {
                source: "decorative-wave.svg"
                Accessible.ignored: true
            }
        }

        ColumnLayout {
            spacing: 4
            Label {
                text: qsTr("Email address")
            }
            TextField {
                Layout.fillWidth: true
                placeholderText: qsTr("name@example.org")
            }
        }

        Label {
            id: statusLine
            text: qsTr("Saved %1 files").arg(savedCount)
            Accessible.role: Accessible.StatusBar
            Accessible.name: text
        }
    }
}
```

## Keeping the accessibility tree clean

- **`a11y-hidden-exposed` (Error, runtime):** Hiding a pane with `opacity: 0`, zero size, or clipping leaves its controls in the accessibility tree and reachable by Tab. Bind `visible: opacity > 0` so invisible items leave the tree.
- **`a11y-ignored-focusable` (Error, static and runtime):** Never set `Accessible.ignored: true` on an item that takes focus or contains focusable children.
- **`a11y-nested-interactive` (Warning, static and runtime):** Do not place a `Button` inside a clickable `ItemDelegate`. Place sibling controls side by side in a `RowLayout` instead.
- **`a11y-set-size` (Warning, runtime):** Use `ListView` rather than a hand-rolled `Repeater` inside a `Flickable` so virtualised delegates automatically report their position and total count (for example, "3 of 120").
- **`a11y-current-unmarked` (Warning, runtime):** Bind `highlighted: ListView.isCurrentItem` on list delegates so the active row is exposed to assistive technologies and stands out visually by at least 3:1 (`limits::NONTEXT_CONTRAST`).

```qml
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ApplicationWindow {
    width: 360
    height: 220
    visible: true
    title: qsTr("Files")

    ListView {
        anchors.fill: parent
        currentIndex: 0
        model: ListModel {
            ListElement { name: "README.md" }
            ListElement { name: "Cargo.toml" }
        }

        delegate: RowLayout {
            required property string name
            required property int index
            width: ListView.view.width
            spacing: 4

            ItemDelegate {
                Layout.fillWidth: true
                text: name
                highlighted: ListView.isCurrentItem
            }

            Button {
                text: qsTr("Delete %1").arg(name)
            }
        }
    }
}
```

## Pointer target sizes

- **`a11y-target-size` (Error, runtime):** Every interactive target must be at least `limits::TARGET_MIN_DESKTOP` (24 px on desktop, matching WCAG 2.2 SC 2.5.8; Apple's macOS default is 28 pt) and `limits::TARGET_MIN_TOUCH` (44 px on touch). If an icon graphic is 16×16 px, keep `icon.width: 16` and `icon.height: 16` on a `ToolButton` whose `implicitWidth` and `implicitHeight` are at least 28 px.

## Focus and keyboard reach

Every action that works with a pointer must also work from a keyboard, with a visible focus ring and predictable key bindings.

### Standard keyboard behaviour by widget

Tab moves between widgets; arrow keys move inside composite widgets:

| Widget | Tab stops | Inside the widget | Activate | Close |
|---|---|---|---|---|
| `Button` | One stop | — | Space, Enter | — |
| `CheckBox`, `Switch` | One stop | — | Space | — |
| Radio group | One stop (on checked item) | Arrow keys move and check | — | — |
| `TabBar` | One stop (on current tab) | Left/Right (wrap), Home/End | Follows focus, or Enter/Space | — |
| `ListView`, tree | One stop | Up/Down, Home/End; Right/Left for tree nodes | Enter | — |
| `Menu` | — | Up/Down (wrap), Right/Left for submenus | Enter | Escape |
| `ComboBox` | One stop | Up/Down change value; Alt+Down opens | Enter | Escape |
| `Slider` | One stop | Arrow keys step, Page Up/Down big step, Home/End | — | — |
| `Dialog` | Tab cycles inside | — | Enter on default button | Escape |

### Keyboard and focus rules

- **`a11y-tab-unreachable` (Error, runtime) and `focus-pointer-only` (Warning, static):** Every pointer-operated control must be reachable by Tab (`activeFocusOnTab: true`) and respond to `Keys.onReturnPressed` and `Keys.onSpacePressed`. Use standard controls (`Button`, `ItemDelegate`) instead of bare `MouseArea`s.
- **`a11y-tab-trap` (Error, runtime):** Tab must be able to leave every control it enters. In a `TextArea` where Tab would otherwise insert a tab character, set `KeyNavigation.priority: KeyNavigation.BeforeItem` and `KeyNavigation.tab` to the next control.
- **`focus-indicator-missing` (Error, runtime):** Every focusable control must draw a visible focus indicator on `visualFocus` (not `activeFocus`) with at least 3:1 contrast (`limits::NONTEXT_CONTRAST`).
- **`focus-order` (Warning, runtime):** Declare items in reading order so pressing Tab does not jump backwards up a column or against the reading direction.
- **`focus-hover-only` (Warning, static):** Actions revealed on hover (such as a row button) must also be visible when `visualFocus` or `highlighted` is true.
- **`focus-popup-stranded` (Warning, runtime) and `focus-escape-dismiss` (Warning, runtime):** Modal popups must take focus (`focus: true`) and close when the user presses Escape (`closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside`).
- **`focus-shortcut-under-modal` (Warning, runtime):** Disable window-level `Shortcut` items while a modal dialog is open (`enabled: !confirmDialog.visible`).
- **`focus-arrow-keys` (Hint, runtime):** Give a `ListView` a single Tab stop (`activeFocusOnTab: true`) and set `focusPolicy: Qt.NoFocus` on delegate controls so users do not have to press Tab through every row.
- **`focus-destructive-undo` (Hint, static):** Destructive buttons (`Delete`, `Remove`, `Discard`, `Erase`, `Clear`) should open a confirmation dialog or provide an undo action rather than destroying data immediately on the first click.

```qml
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ApplicationWindow {
    width: 400
    height: 260
    visible: true
    title: qsTr("Compose note")

    Shortcut {
        sequence: StandardKey.Close
        enabled: !discardDialog.visible
        onActivated: close()
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 16
        spacing: 12

        TextArea {
            id: bodyField
            Layout.fillWidth: true
            Layout.fillHeight: true
            Accessible.name: qsTr("Note body")
            KeyNavigation.priority: KeyNavigation.BeforeItem
            KeyNavigation.tab: discardButton
        }

        RowLayout {
            Layout.alignment: Qt.AlignRight
            spacing: 8

            Button {
                id: discardButton
                text: qsTr("Discard note")
                onClicked: discardDialog.open()
            }

            Button {
                id: saveButton
                text: qsTr("Save note")
                highlighted: true
                background: Rectangle {
                    color: palette.button
                    border.width: saveButton.visualFocus ? 2 : 0
                    border.color: palette.highlight
                    radius: 4
                }
            }
        }
    }

    Dialog {
        id: discardDialog
        title: qsTr("Discard note?")
        modal: true
        focus: true
        standardButtons: Dialog.Cancel | Dialog.Discard
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside

        Label {
            text: qsTr("Your unsaved changes will be lost.")
        }
    }
}
```

## Contrast and visual states

Accessibility also depends on sufficient visual contrast across light, dark, and high-contrast schemes:

- Normal text requires at least **4.5:1** contrast against its composited background, and large text (24 px or 18.66 px bold) requires **3:1** (`color-text-contrast`).
- Control borders, icons, focus rings, and state indicators require at least **3:1** contrast (`color-nontext-contrast`).
- Never communicate a state such as online/offline or checked/unchecked by colour alone; pair colour with a label, icon, or shape change (`color-state-only`).

See [Colour and contrast](color.md) for the full set of colour rules and palette guidelines.

## See also

- [Design guardrails](design-guardrails.md)
- [Colour and contrast](color.md)
- [Motion and reduced motion](motion.md)
- [Layout, content, and typography](layout-and-typography.md)
- [First app](../start/first-app.md)
