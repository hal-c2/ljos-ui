---
title: Value Inputs
description: Collect numbers, choices, and text using Slider, RangeSlider, Dial, SpinBox, ComboBox, TextField, TextArea, and ProgressBar.
section: controls
order: 3
---
# Value Inputs

Use the value input controls when the user selects a number from a range, picks an item from a list, types text, or watches progress.

## `Slider`, `RangeSlider`, `Dial`, and `ProgressBar`

`Slider` holds a `value` between `from` (`0` by default) and `to` (`1` by default). It starts at `value: 0` with `stepSize: 0`, `snapMode: 0`, `live: true`, `orientation: 1` (horizontal), and `focusPolicy: 11`. Its `handle` follows `value` (so `0.5` places the handle halfway along the track), and `position` and `visualPosition` report its relative place.

Pressing on the track or dragging the handle moves `value` to the pointer position, clamped to the ends of the track. With `live: false`, `value` updates only when the pointer is released. Setting `snapMode` to `Slider.SnapAlways` snaps during the drag, while snap-on-release snaps when let go. Calling `valueAt()` with a `stepSize` (such as `0.1`) returns the nearest step. Arrow keys step the slider by `stepSize` (and step in reverse when `mirrored` is true); a vertical slider grows upward from its bottom end. The mouse wheel steps the slider only when `wheelEnabled` is true.

`RangeSlider` provides two handles, `first` and `second`, with `first.value` and `second.value` representing shares of the track between `from` and `to`. Pressing the track moves the nearer handle, and neither handle can be dragged past the other.

`Dial` defaults to `from: 0`, `to: 1`, `value: 0`, a `startAngle` to `endAngle` sweep of `-140` to `140` degrees (`angle` reflects the current angle), and an implicit size of `184 × 184`. By default, dragging around the centre updates `value` from the pointer's angle (pressing at the exact centre sets `0.5`, twelve o'clock); with `inputMode` set to `Dial.Horizontal` or `Dial.Vertical`, the pointer instead moves along a line, twice the dial's width or height for the whole range. Setting `wrap: true` lets the dial pass its end and wrap around to the start, and `live: false` commits `value` on release. It also exposes `stepSize`, `snapMode`, and `inputMode`.

`ProgressBar` displays progress between `from` (`0`) and `to` (`1`). It starts with `indeterminate: false`, an implicit size of `200 × 6`, a `#e4e4e4` background, and `focusPolicy: 0` so it never takes keyboard focus. Its fill tracks `position` (filling from the right when mirrored), and setting `indeterminate: true` preserves its `value`.

Write this as `Main.qml` and run `ljos check` on its directory:

```qml
import QtQuick
import QtQuick.Controls.Basic

Column {
    padding: 12
    spacing: 12

    Slider {
        id: volume
        from: 0
        to: 1
        value: 0.25
        stepSize: 0.1
    }

    ProgressBar {
        from: 0
        to: 1
        value: volume.value
    }

    Dial {
        from: 0
        to: 1
        value: 0.5
    }

    Text {
        text: "Volume " + Math.round(volume.value * 100) + " %"
    }
}
```

The slider starts at a quarter and steps by a tenth; the progress bar follows `volume.value`, and the text reads the percentage.

## `ComboBox`

`ComboBox` presents a drop-down list of rows from its `model`. With no model, `count` is `0`, `currentIndex` is `-1`, `currentText` is `""`, and `value` is `undefined`. When populated, it displays the text of `currentIndex`, using `textRole` and `valueRole` to pick which role of each row is shown (`displayText`, `currentText`) and which becomes `currentValue`. Inserting a row before `currentIndex` keeps the numeric index and changes the current row; removing the last row while it is current leaves the index pointing at no row. A binding on `currentIndex` stays intact as the model grows and shrinks.

Pressing on a `ComboBox` does not open the popup until you release the pointer over the combo box; releasing outside opens nothing. Hovering over a row highlights it, and the mouse wheel steps through rows only when `wheelEnabled` is enabled.

When `editable` is true, `ComboBox` holds a text field whose `editText` starts as the current row's text and follows it. Typing a prefix of an existing row completes it (and you can search rows with `find()`), while pressing Return emits `accepted` (choosing an item emits `activated`). Typing text that matches no row leaves no row current.

```qml
import QtQuick
import QtQuick.Controls.Basic

Column {
    padding: 12
    spacing: 8

    ComboBox {
        id: colours
        model: ["Red", "Green", "Blue"]
        currentIndex: 1
    }

    Text {
        text: "Current: " + colours.currentText
    }
}
```

## `SpinBox`, `TextField`, and `TextArea`

`SpinBox` edits an integer between `from` (`0`) and `to` (`99`), starting with `value: 0`, `stepSize: 1`, `editable: false`, `wrap: false`, `live: false`, and `displayText: "0"`. Values outside `from` and `to` are clamped to the range. If you give `from` above `to`, the range still works as the span between them: values are clamped to it, `increase()` moves towards `to`, and with `wrap: true` it wraps from `to` back to `from`.

Holding an up or down button on a `SpinBox` steps repeatedly. By default, numbers are formatted with the locale's digit grouping (for example, `1234567` shows as `"1,234,567"` in English and `"1.234.567"` in German), or you can supply `textFromValue`, `valueFromText`, and `validator`. When `editable` is true, the `SpinBox` contains a `TextInput`: digits that would take the number out of range are refused as you type, and typed numbers do not update `value` until you press Return, Enter, or Tab— UNLESS `live` is true, in which case every keystroke updates `value`. Watch out when mixing typing and buttons on a non-live `SpinBox`: clicking the up or down button discards uncommitted typed text.

`TextField` wraps a single-line `TextInput` with a `background` and `placeholderText`. In the Basic style, an empty `TextField` is `200 × 40` with `padding: 6` and `leftPadding: 10`; with `background: null`, it sizes itself to the wider of its `text` or `placeholderText` and the height of one line plus padding. `placeholderText` remains visible while `text` is empty (even when the field has keyboard focus) and hides as soon as text is entered. Clicking (press and release) or pressing and dragging gives the field keyboard focus—a press alone without movement or release does not. Double-clicking selects a word, dragging selects a range (`selectedText`, `selectByMouse`), and text wider than the field scrolls horizontally while clipping to the field's content area (leaving the `background` under the padding intact). A `readOnly` field still accepts keyboard focus (`activeFocusOnTab`) but hides the cursor.

```qml
import QtQuick
import QtQuick.Controls.Basic

Column {
    padding: 12
    spacing: 8

    TextField {
        width: 200
        placeholderText: "Name"
        text: "Ljós"
    }

    SpinBox {
        from: 0
        to: 99
        value: 5
        editable: true
    }
}
```

`TextArea` wraps a multi-line `TextEdit` with a `background` and `placeholderText`. Its implicit size grows with its text, placeholder, and line count. When `wrapMode` is set (such as `TextEdit.Wrap`), long lines wrap, and when placed as the single child of a `ScrollView`, the `TextArea` matches the view's width and scrolls vertically. Keep in mind that pressing the Tab key inside a focused `TextArea` inserts a tab character rather than moving focus to the next control.

```qml
import QtQuick
import QtQuick.Controls.Basic

Column {
    width: 240
    padding: 12
    spacing: 8

    TextArea {
        width: parent.width - 24
        placeholderText: "Notes"
        wrapMode: TextEdit.Wrap
    }
}
```

## Differences from Qt

- A `ComboBox` popup with more rows than fit in the window is limited to the window's height and scrolls, and popups run their `enter` and `exit` transitions as Qt's do.
- In the Basic style, `palette.placeholderText` has a `3.04:1` contrast ratio on white, which is below the `4.5:1` threshold for normal text; set `palette.placeholderText` when you need higher contrast.

## See also

- [Quick Controls Overview](./overview.md)
- [Containers and Popups](./containers-and-popups.md)
- [Localisation](./localisation.md)
