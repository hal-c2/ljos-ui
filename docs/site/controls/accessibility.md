---
title: Accessibility
description: Expose roles, names, states, and values to assistive technologies and respond to system contrast and motion preferences.
section: controls
order: 7
---
# Accessibility

Use the `Accessible` attached properties when you need to give an icon-only button a spoken label, add a description to a control, override a default role, or read the user's system accessibility preferences.

## Default roles, names, and overrides

Every control in Ljós exposes an accessibility node for itself with a default `Accessible.role` and `Accessible.name` derived from its type and text, without extra markup:

- `Button` → `button`
- `CheckBox` and `CheckDelegate` → `checkbox`
- `RadioButton` → `radiobutton`
- `Switch` and `SwitchDelegate` → `switch`
- `Label` → `label`
- `GroupBox` → `group` (controls inside the group box appear inside its group node in the accessibility tree)
- `ToolBar` → `toolbar` (buttons in the tool bar appear inside its node)

A control's accessible name follows its `text` or `title` dynamically: changing `text` updates the name immediately. On buttons (including check boxes and delegates), a single ampersand (`&`) that marks a mnemonic is stripped from the accessible name, while a doubled ampersand (`&&`) becomes a single `&`; a lone trailing `&` stays. Controls that are not buttons, such as `Label` and `GroupBox`, keep ampersands in their accessible name. The mnemonic letter is drawn underlined, and Alt with that letter clicks the button. Screen readers and accessibility tools can locate a control by its role and name and trigger clicks or toggle check boxes directly.

You can override the defaults on any item using `Accessible` attached properties (`QtQuick.AccessibleAttached`), including `role`, `name`, `description`, `focusable`, `focused`, `ignored`, `modal`, `checkable`, `checked`, `checkStateMixed`, `defaultButton`, `editable`, `multiLine`, `passwordEdit`, `pressed`, `readOnly`, `searchEdit`, `selectable`, `selectableText`, `selected`, and `expandable` (plus `Accessible.AnnouncementPoliteness` values `Polite` and `Assertive`, and `Accessible.Role` values such as `NoRole`, `TitleBar`, `MenuBar`, `ScrollBar`, `Grip`, `Sound`, `Cursor`, `Caret`, and `AlertMessage`).

Write this as `Main.qml` and run `ljos check` on its directory:

```qml
import QtQuick
import QtQuick.Controls.Basic

Column {
    padding: 12
    spacing: 12

    Slider {
        from: 0
        to: 10
        value: 4
        Accessible.name: "Volume"
        Accessible.description: "Sets the loudness"
    }

    Button {
        text: "Save"
        Accessible.name: "Save the document"
    }
}
```

Here the slider reports its name as `"Volume"`, its value as `4` in the range `0` to `10`, and its description, while the button reports `"Save the document"` instead of `"Save"`.

Watch out for icon-only buttons: a control with no `text` has no accessible name at all, regardless of the icon it displays. Always set `Accessible.name` on icon-only buttons:

```qml
import QtQuick
import QtQuick.Controls.Basic

Row {
    padding: 12
    spacing: 8

    ToolButton {
        icon.name: "document-save"
        Accessible.name: "Save file"
    }

    ToolButton {
        text: "Undo"
    }
}
```

## Values, states, and popups in the accessibility tree

Value and state controls keep the accessibility tree updated as the user interacts with them:

- **Check states**: Checked, unchecked, and partially checked (`checkStateMixed`) states on `CheckBox` and `CheckDelegate` are reported, along with pressed, expanded, disabled, read-only, and selected states, and the position of a `ComboBox` or `Menu` row within its set.
- **Sliders, dials, and progress bars**: `Slider`, `Dial`, and `ProgressBar` report their current numeric value and range. On a `RangeSlider`, each handle exposes its own value and can be moved independently by an assistive technology without moving the other handle (stopping when it meets the other handle).
- **Dialogs, popups, menus, and tool tips**: While a `Dialog`, `Popup`, `Menu`, or `ToolTip` is closed, it is absent from the accessibility tree. When a `Dialog` opens, it enters the tree as a `dialog` node named after its `title`, holds its child controls inside its node, and moves keyboard focus inside the dialog until it closes (a plain `Popup` enters as an unnamed `dialog` node). Activating a dialog button or `MenuItem` from a screen reader triggers the item and closes the dialog with its result. An open `Menu` is named by its `title`, and a visible `ToolTip` is named by its `text`.

```qml
import QtQuick
import QtQuick.Controls.Basic

Item {
    width: 300
    height: 200

    Dialog {
        id: confirm
        title: "Save?"
        standardButtons: Dialog.Ok | Dialog.Cancel
        modal: true

        Label {
            text: "Save the changes before closing."
        }
    }

    Button {
        text: "Close"
        onClicked: confirm.open()
    }
}
```

## System preferences and reduced motion

You can inspect system accessibility preferences through `Qt.styleHints.accessibility`:

- `Qt.styleHints.accessibility.contrastPreference` and `Qt.styleHints.accessibility.motionPreference` read the system's contrast and motion settings.
- When reduced motion is active, movement animations jump straight to their end state (still emitting `started` and `finished`), fades and colour transitions are capped at `150` ms, and animations that run for ever keep running. Qt documents `motionPreference` for a release later than 6.11, so Ljós provides it ahead of Qt.
- Ljós also provides `fontScale`, `screenReaderActive`, and `font.scaleWithSystem` as extensions beyond Qt's standard style hints.

## Differences from Qt

- Default control roles and states are implemented from Qt's specification rather than compared against Qt at run time, because Qt does not expose default assistive-technology roles to QML queries.
- `fontScale`, `screenReaderActive`, and `font.scaleWithSystem` are Ljós extensions.
- The Basic style's default `palette.placeholderText` (`3.04:1` on white, `3.95:1` in the dark scheme) matches Qt's value but is below the `4.5:1` minimum for normal text; set `palette.placeholderText` when you need higher contrast.

## See also

- [Quick Controls Overview](./overview.md)
- [Buttons and Toggles](./buttons-and-toggles.md)
- [Value Inputs](./value-inputs.md)
- [Containers and Popups](./containers-and-popups.md)
