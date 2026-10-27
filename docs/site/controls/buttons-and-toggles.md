---
title: Buttons and Toggles
description: Trigger actions and toggle state with Button, ToolButton, RoundButton, CheckBox, RadioButton, Switch, ButtonGroup, and DelayButton.
section: controls
order: 2
---
# Buttons and Toggles

Use the button and toggle controls when the user triggers an action, turns a setting on or off, or chooses from a group of options. All of them build on `AbstractButton`, so they share press handling, icons, keyboard activation, and checkable state.

## Shared `AbstractButton` behaviour and icons

`AbstractButton` draws nothing by itself, but provides the properties, signals, and methods shared by every button control:

- **State and display**: `text`, `display` (`AbstractButton.IconOnly` `0`, `TextOnly` `1`, `TextBesideIcon` `2`, `TextUnderIcon` `3`), `down`, `pressed`, `pressX`, `pressY`, `checked`, `checkable`, `autoExclusive`, `indicator`, and `action`
- **Auto-repeat**: `autoRepeat` (off by default), `autoRepeatDelay` (`300` ms), and `autoRepeatInterval` (`100` ms)
- **Signals**: `pressed`, `released`, `canceled`, `clicked`, `toggled`, `doubleClicked`, and `pressAndHold`
- **Methods**: `click()` and `toggle()`

By default, an `AbstractButton` starts unchecked, not checkable, and with `display` set to `AbstractButton.TextBesideIcon` (`2`).

Clicking an enabled button emits `pressed`, `released`, and `clicked` in order, leaving `pressed` false afterwards; on a checkable button, `toggled` is emitted after `pressed` and before `released` and `clicked`, and a second click unchecks it. Clicking a disabled button does nothing, and disabling a button while it is pressed cancels the press.

While the pointer is held down on a button, both `pressed` and `down` are true. Moving the pointer off the button releases `down`, moving back presses it again, and releasing outside the button emits `canceled` instead of `clicked`. When a button has an `onDoubleClicked` handler, two clicks about `100` ms apart emit `doubleClicked` on the second click instead of `clicked`; clicks a second apart, and quick clicks on a button with no such handler, emit two `clicked` signals. Holding the pointer for a second emits `pressAndHold`, and releasing afterwards does not emit `clicked`. When `autoRepeat` is true, holding the button emits the first repeat click `300` ms after the press and then every `100` ms until release; releasing before the first repeat produces a single click.

`hovered` is true while the pointer is over the button unless `hoverEnabled` is false. Buttons show their icon beside their text by default: `icon.source` names the image, `icon.width` and `icon.height` bound its size, `icon.cache` controls caching, and `icon.color` defaults to `#00000000`, which makes the icon follow the text colour.

Watch out for these subtleties:

- Calling `click()` from QML runs the same steps as a user click, including checking a checkable button. Calling `toggle()` flips `checked` without emitting `clicked`, even on a button whose `checkable` is false. Setting `down` from a script does not change `pressed`.
- Setting `icon.name` without `icon.source` shows nothing because Ljós does not bundle a system icon theme.
- A `checked` binding on a button is preserved even when a click handler writes back to the bound property.

## `Button`, `ToolButton`, and `RoundButton`

`Button` adds `flat` and `highlighted`. In the Basic style, a `Button` with no text matches its `100 × 40` background, with `padding: 6`, `horizontalPadding: 8`, and `spacing: 6`; longer text grows its implicit width. A normal Basic `Button` has a `#e0e0e0` background when enabled, `#cfcfcf` when down, `#353637` when checked or highlighted, and `#797a7a` when highlighted and held down. A `flat` button hides its background unless it is down, checked, or highlighted, draws its text in the window text colour (or the highlight colour when it has keyboard focus), and switches to bright text when checked or highlighted.

`ToolButton` is a square `40 × 40` control when it has no text, with `padding: 6` and a `24 × 24` icon, growing wider for long text. `RoundButton` is a button whose `radius` defaults to half its shorter side; if you set `radius` explicitly it keeps that value when resized, and resetting `radius` to `undefined` makes it round again.

You can replace `background` or `contentItem` on a `Button` (or a component derived from `Button`); the style's default part is then not created, your replacement becomes a child of the button and can read states such as `down`, and a larger `contentItem` determines the button's implicit size (or the padded content alone when `background: null`).

```qml
import QtQuick
import QtQuick.Controls

Column {
    spacing: 12
    padding: 12

    Button {
        text: qsTr("Save")
        highlighted: true
        onClicked: status.text = "Saved"
    }

    ToolButton {
        text: "Bold"
        checkable: true
    }

    RoundButton {
        text: "+"
        radius: 6
    }

    CheckBox {
        text: "Notify me"
        checked: true
    }

    Switch {
        text: "Wi-Fi"
    }

    Text {
        id: status
        text: ""
    }
}
```

Pressing **Save** sets the status text to `"Saved"`, while the other controls toggle when clicked.

## `CheckBox`, `Switch`, `RadioButton`, and `ButtonGroup`

`CheckBox` tracks `checkState` across `Qt.Unchecked` (`0`), `Qt.PartiallyChecked` (`1`), and `Qt.Checked` (`2`). By default, clicking toggles between unchecked and checked, while setting `checked` in script does not emit `clicked`. When `tristate` is true, clicks cycle from unchecked to partially checked, then checked, then unchecked again; a partially checked box has `checked: false`, and calling `toggle()` skips the partial state. When `tristate` is false, clicking a partially checked box checks it. You can also assign a function to `nextCheckState` to control what state a click selects next.

`Switch` is checkable and starts with `checked: false` and `position: 0` (along with `visualPosition`). Clicking flips it between `0` and `1`. You can also drag the handle: `position` follows the pointer and clamps to the ends; releasing past `0.5` turns the switch on, while releasing at or before `0.5` leaves it off. When the switch is mirrored, its handle travels in the opposite direction.

`RadioButton` starts checkable, unchecked, and with `autoExclusive: true`. Among `autoExclusive` buttons that share the same parent item, only one can be checked at a time: checking one unchecks the others, clicking an already checked radio button leaves it checked, and radio buttons in a different parent form a separate group.

When you want to group buttons across different parents, or inspect the group as a whole, attach them to a `ButtonGroup` using `ButtonGroup.group` (or `addButton(button)` and `removeButton(button)`):

```qml
import QtQuick
import QtQuick.Controls

Column {
    padding: 12
    spacing: 8

    ButtonGroup {
        id: size
    }

    RadioButton {
        text: "Small"
        ButtonGroup.group: size
    }

    RadioButton {
        text: "Medium"
        checked: true
        ButtonGroup.group: size
    }

    RadioButton {
        text: "Large"
        ButtonGroup.group: size
    }

    Text {
        text: "Chosen: " + (size.checkedButton ? size.checkedButton.text : "none")
    }
}
```

`ButtonGroup` is `exclusive: true` by default, lists its members in `buttons`, reports the active button in `checkedButton` (`null` when none is checked, and setting `checkedButton = null` unchecks the active button), and emits `clicked(button)` when any member is clicked. Adding an already checked button makes it `checkedButton`; removing the checked button unchecks it and resets `checkedButton` to `null`. When `exclusive` is false, `ButtonGroup.checkState` is `1` when some buttons are checked and `2` when all are checked, and assigning `2` or `0` to `checkState` checks or unchecks every button in the group.

## `DelayButton`

Use `DelayButton` for destructive or irreversible actions that require the user to hold the button down before they trigger. It starts checkable with `delay: 300` (in milliseconds) and `progress: 0`.

While held down, `progress` rises from `0` to `1` over `delay` ms; reaching `1` emits `activated`, and releasing afterwards leaves the button `checked` and emits `clicked`. If you release the button before `delay` elapses, `progress` runs back down to `0` and neither `activated` nor `checked` occurs. A click on a checked `DelayButton` unchecks it without waiting for `delay`, and `progress` is `0` after the release. Setting `checked` from script sets `progress` to `1` (or `0` when unchecked).

```qml
import QtQuick
import QtQuick.Controls

Column {
    padding: 12
    spacing: 8

    DelayButton {
        id: remove
        text: "Hold to delete"
        delay: 1000
        onActivated: status.text = "Deleted"
    }

    Text {
        id: status
        text: "Hold the button for a second"
    }
}
```

Watch out for keyboard activation on `DelayButton`: pressing and immediately releasing the Space key emits `pressed`, `released`, and `clicked`, but does not emit `activated` or check the button.

## Differences from Qt

- Pointer-held interactions (`pressed` while held, `pressAndHold`, `autoRepeat`, and `canceled`) are covered by Ljós-only scenarios: the comparison tool sends whole clicks, so what happens between press and release was read from Qt with its own test tool instead.
- Material, Fusion, and Universal restyle a subset of these controls (for example, Material provides its own `Button`, `CheckBox`, `RadioButton`, `RoundButton`, and `Switch`) and inherit the remaining controls from Basic.

## See also

- [Quick Controls Overview](./overview.md)
- [Styles](./styles.md)
- [Accessibility](./accessibility.md)
