---
title: Quick Controls Overview
description: Learn how Qt Quick Controls work in Ljós, including the Control base type, sizing, padding, palettes, fonts, and built-in styles.
section: controls
order: 1
---
# Quick Controls Overview

Use `QtQuick.Controls` when you need ready-made user interface controls—buttons, text fields, sliders, menus, panes, and windows—or when you want to build your own controls on the `Control` base type. You can import `QtQuick.Controls` to let Ljós pick the active style, or import `QtQuick.Controls.Basic`, `QtQuick.Controls.Fusion`, `QtQuick.Controls.Material`, or `QtQuick.Controls.Universal` directly to lock a file to that style.

## Styles and control catalogue

Every control exists in every style module, so a `SpinBox` or `ComboBox` works the same way whether your program runs under Basic, Fusion, Material, or Universal.

The Basic style is the neutral foundation that the other styles and custom controls build on. When a style does not provide its own version of a control, it falls back to Basic unless another fallback style is named. If your program does not choose a style, it uses the platform style, which is Fusion on Linux.

When a program starts, Ljós selects the style in this order:

1. The `-style` command-line option
2. The `QT_QUICK_CONTROLS_STYLE` environment variable
3. The `[Controls] Style=` line in the configuration file that `QT_QUICK_CONTROLS_CONF` names
4. The platform style (Fusion on Linux)

Importing a style module by name in QML overrides the startup selection for that file. You can also supply a custom style as a module of your own application; its controls replace those of the same name in `QtQuick.Controls`, while any controls it omits come from its fallback style.

Each built-in style brings its own metrics:

- **Basic**: neutral light and dark palettes and base control shapes
- **Fusion**: desktop metrics, such as an `80 × 24` `Button` with `4` px padding
- **Material**: `64 × 52` `Button` and `120 × 56` `TextField`, with attached `Material.theme` and `Material.accent` properties
- **Universal**: `32 × 32` `Button`, with attached `Universal.theme` and `Universal.accent` properties

The Basic style module (`QtQuick.Controls.Basic`) provides 61 types: `ApplicationWindow`, `BusyIndicator`, `Button`, `CheckBox`, `CheckDelegate`, `ComboBox`, `Control`, `DayOfWeekRow`, `DelayButton`, `Dial`, `Dialog`, `DialogButtonBox`, `DoubleSpinBox`, `Drawer`, `Frame`, `GroupBox`, `HorizontalHeaderView`, `HorizontalHeaderViewDelegate`, `ItemDelegate`, `Label`, `Menu`, `MenuBar`, `MenuBarItem`, `MenuItem`, `MenuSeparator`, `MonthGrid`, `Page`, `PageIndicator`, `Pane`, `Popup`, `ProgressBar`, `RadioButton`, `RadioDelegate`, `RangeSlider`, `RoundButton`, `ScrollBar`, `ScrollIndicator`, `ScrollView`, `SelectionRectangle`, `Slider`, `SpinBox`, `SplitView`, `StackView`, `SwipeDelegate`, `SwipeView`, `Switch`, `SwitchDelegate`, `TabBar`, `TabButton`, `TableViewDelegate`, `TextArea`, `TextField`, `ToolBar`, `ToolButton`, `ToolSeparator`, `ToolTip`, `TreeViewDelegate`, `Tumbler`, `VerticalHeaderView`, `VerticalHeaderViewDelegate`, and `WeekNumberColumn`. In addition, `QtQuick.Controls` provides `Calendar`, `ContextMenu`, and `ContextMenuAttached`.

## The `Control` base type, sizing, and padding

`Control` is the base type that controls build on. It places a `background` item behind the content and a `contentItem` in front, with `padding` around the content and `topInset`, `leftInset`, `rightInset`, and `bottomInset` around the background.

A control's implicit size is the larger of its background (`implicitBackgroundWidth` by `implicitBackgroundHeight`) and its padded content (`implicitContentWidth` plus horizontal padding by `implicitContentHeight` plus vertical padding). For example, a background of `80 × 30` and content of `40 × 50` with `padding: 10` give an implicit size of `80 × 70`. When the control itself is sized, `contentItem` and `background` follow the control's size, with the content reduced by the padding: a `200 × 100` control with `padding: 10` gives a content size of `180 × 80`. `availableWidth` and `availableHeight` report the room left for the content and never drop below `0`.

Padding follows a three-level priority:

- A side-specific property (`topPadding`, `leftPadding`, `rightPadding`, `bottomPadding`) wins over its axis padding.
- Axis padding (`horizontalPadding`, `verticalPadding`) covers two sides each and wins over `padding`.
- `padding` sets the fallback for all four sides.

A `Control` with no parts starts with an implicit size of `0 × 0`, `padding: 0`, `spacing: 0`, and `hovered: false`. It also exposes `hoverEnabled`, `wheelEnabled`, `mirrored` (true when `LayoutMirroring.enabled` is set), `locale`, `focusPolicy`, `focusReason`, and `visualFocus`. For controls that accept keyboard focus via `focusPolicy`, the Basic style draws a frame in the highlight colour around the focused control.

You can build your own control by placing a `Badge.qml` file beside `Main.qml`:

```qml
import QtQuick
import QtQuick.Controls

Control {
    id: root
    property alias text: label.text

    padding: 6
    palette.button: "#2d6cdf"
    palette.buttonText: "white"

    background: Rectangle {
        radius: 4
        color: root.palette.button
    }

    contentItem: Text {
        id: label
        color: root.palette.buttonText
        font.pixelSize: 14
    }
}
```

```qml
import QtQuick
import QtQuick.Controls

Column {
    padding: 12
    spacing: 8

    Badge {
        text: "New"
    }

    Pane {
        palette.window: "#fff8e1"

        Button {
            text: "Inside the pane"
        }
    }

    Button {
        text: "Outside the pane"
    }
}
```

The badge draws a blue rounded box with white text. The button inside the pane inherits the pane's window colour, while the button outside keeps the Basic palette.

Watch out for these details when working with `Control` parts:

- Assigning a new `contentItem` swaps the content: the previous item is hidden and its parent is cleared before the new item is placed inside the padding.
- Disabling a control (`enabled: false`) disables both its `background` and its `contentItem`.

## Palettes and fonts

A control draws with its `palette` (`QtQuick.Palette`), which provides roles such as `window`, `windowText`, `base`, `alternateBase`, `text`, `button`, `buttonText`, `brightText`, `light`, `midlight`, `mid`, `dark`, `shadow`, `highlight`, `highlightedText`, `link`, `linkVisited`, `toolTipBase`, `toolTipText`, `placeholderText`, and `accent`.

In the light Basic palette, `window` is `#ffffff`, `windowText` is `#26282a`, `button` is `#e0e0e0`, `buttonText` is `#26282a`, `highlight` is `#0066ff`, `highlightedText` is `#090909`, and `placeholderText` is `#88353637`. When `Qt.styleHints.colorScheme` is dark (from the system or the application), the Basic style switches to its dark palette (for example, `window` `#000000`), while keeping any colours your application explicitly set.

Palettes also have three colour groups (`palette.active`, `palette.inactive`, and `palette.disabled`). Setting a colour directly on `palette` sets it in every group, while reading `palette.button` on a disabled control reads the disabled group, where colours are paler (such as `buttonText` `#4d26282a` and `base` `#d6d6d6`).

Controls inherit both `palette` colours and `font` attributes (`pixelSize`, `bold`, `italic`, `underline`, `strikeout`, `family`, `weight`) one by one from the item, `Pane`, `Popup`, or window that holds them—including plain `Item` or `Row` parents and controls created later by a `Loader`. Any colour or font attribute a control sets on itself is kept, and bound colours update children whenever the binding changes. Increasing a control's font size also grows its implicit height to fit the text.

```qml
import QtQuick
import QtQuick.Controls

Pane {
    width: 240
    height: 120
    font.pixelSize: 20
    font.italic: true

    Column {
        spacing: 8

        Label {
            text: "Inherits 20 px and italic"
        }

        Button {
            text: "Also 20 px, and bold"
            font.bold: true
        }

        Text {
            text: "A plain Text keeps its own font"
        }
    }
}
```

Here the `Label` and the `Button` inherit `20` px and italic from the `Pane`, and the `Button` adds `font.bold: true`.

Keep a common pitfall in mind: a plain `Text` item does not inherit the `font` of the `Pane` or control it sits inside. Use `Label` when you want text to follow control font and palette inheritance.

## Differences from Qt

- Ljós has no macOS, Windows, or iOS style; the native macOS and Windows styles are deferred, and the platform style on Linux is Fusion.
- Material's `variant` and `shade()` are not supported, and Fusion, Material, and Universal use Basic's shapes in each style's sizes and colours rather than full design-system visuals.
- `ToolTip` uses one shared instance per window. Popups play their `enter` and `exit` transitions (fading instead of moving under reduced motion), and a `Menu` or `ComboBox` popup with more rows than fit scrolls.
- In the Basic style, `palette.placeholderText` has a `3.04:1` contrast ratio on white (matching Qt's palette value); set `palette.placeholderText` explicitly if your application requires WCAG AA contrast.
- A plain `Item` gives child controls the active style's default colours when nothing above sets a colour, whereas Qt on the desktop reads the platform palette instead.

## See also

- [Buttons and Toggles](./buttons-and-toggles.md)
- [Value Inputs](./value-inputs.md)
- [Containers and Popups](./containers-and-popups.md)
- [Styles](./styles.md)
- [Windows](./windows.md)
- [Accessibility](./accessibility.md)
- [Localisation](./localisation.md)
