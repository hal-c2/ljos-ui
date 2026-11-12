---
title: Controls Overview
description: Use Ljós's own controls from Ljos.Controls, or the Basic, Fusion, Material and Universal style modules existing QML imports, and learn the Control base type, sizing, padding, palettes and fonts.
section: controls
order: 1
---
# Controls Overview

Controls are the ready-made parts of an interface: buttons, text fields, sliders, menus, panes and windows. A file imports them from `Ljos.Controls`, which draws them in Ljós's own style, or from `Ljos.Controls.Basic`, which draws them in the Basic style, as most examples on these pages do. You can build your own controls on the `Control` base type in either.

## Ljós's own controls

`Ljos.Controls` draws its controls in Ljós's own style: compact controls, 28 px high with 13 px type and a radius of 8, all drawn with the colour roles of one `Theme` singleton.

```qml
import Ljos.Ui
import Ljos.Layout
import Ljos.Controls

Column {
    spacing: 8
    padding: 12

    Label {
        text: "Ready"
    }

    Button {
        text: "Save"
        ToolTip.text: "Writes the file"
        ToolTip.delay: 500
    }

    Button {
        text: "Send"
        primary: true
    }

    Button {
        text: "Delete"
        destructive: true
    }
}
```

The style has `Button` (with `primary`, `destructive`, `subtle`, `chevron` and `iconName`), `Switch`, `CheckBox`, `ComboBox`, `TextField`, `SpinBox`, `Slider`, `Menu`, `MenuItem`, `MenuSeparator`, `Label`, `ToolTip` and `Icon`, which draws Lucide's icons. `TextArea`, `Dialog`, `ItemDelegate`, `Popup`, `ScrollBar`, `ScrollView`, `FileDialog` and `FolderDialog` are there too, drawn as the Basic style draws them until the style has its own.

Every colour is a role of `Theme` (`canvas`, `surface`, `input`, `border`, `text`, `textMuted`, `accent`, `focus`, `link`, `destructive` and more), and every role is derived from three: `canvas`, `text` and `accent`, which follow `Ljos.Design`'s `Colors` in light and dark. Each derived role is moved until it meets its WCAG contrast ratio against every fill it is drawn on, so a theme built from your own three colours stays legible. Set any role to override it (`Theme.input = "#b00020"`); the others still follow the three.

A pressed button gives a little and a switch's thumb slides, using `Ljos.Design`'s `Motion`. Under reduced motion nothing moves, and the state still shows: a pressed button's fill says it is pressed. A control reached by the keyboard draws a ring in the `focus` role.

A file chooses its style by its import and nothing else, so it draws the same wherever it runs. `Ljos.Controls.Basic` has the same controls drawn in the Basic style, and a file moves between the two by changing its import.

## Style modules

Files that import `QtQuick.Controls` choose among four style modules: Basic, Fusion, Material and Universal. Every control exists in every style module, so a `SpinBox` or `ComboBox` works the same way whether your program runs under Basic, Fusion, Material, or Universal.

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

```qml file=Badge.qml group=badge
import Ljos.Ui
import Ljos.Controls.Basic

Control {
    id: root
    property alias text: label.text

    padding: 6
    palette.button: "#2d6cdf"
    palette.buttonText: "white"

    background: Rectangle {
        radius: 4
        fill: root.palette.button
    }

    contentItem: Text {
        id: label
        color: root.palette.buttonText
        font.size: 14
    }
}
```

```qml group=badge
import Ljos.Ui
import Ljos.Layout
import Ljos.Controls.Basic

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

A control draws with its `palette`, which provides roles such as `window`, `windowText`, `base`, `alternateBase`, `text`, `button`, `buttonText`, `brightText`, `light`, `midlight`, `mid`, `dark`, `shadow`, `highlight`, `highlightedText`, `link`, `linkVisited`, `toolTipBase`, `toolTipText`, `placeholderText`, and `accent`.

In the light Basic palette, `window` is `#ffffff`, `windowText` is `#26282a`, `button` is `#e0e0e0`, `buttonText` is `#26282a`, `highlight` is `#0066ff`, `highlightedText` is `#090909`, and `placeholderText` is `#88353637`. When `Qt.styleHints.colorScheme` is dark (from the system or the application), the Basic style switches to its dark palette (for example, `window` `#000000`), while keeping any colours your application explicitly set.

Palettes also have three colour groups (`palette.active`, `palette.inactive`, and `palette.disabled`). Setting a colour directly on `palette` sets it in every group, while reading `palette.button` on a disabled control reads the disabled group, where colours are paler (such as `buttonText` `#4d26282a` and `base` `#d6d6d6`).

Controls inherit both `palette` colours and `font` attributes (`size`, `weight`, `italic`, `underline`, `strikeout`, `family`) one by one from the item, `Pane`, `Popup`, or window that holds them—including plain `Item` or `Row` parents and controls created later by a `Loader`. Any colour or font attribute a control sets on itself is kept, and bound colours update children whenever the binding changes. Increasing a control's font size also grows its implicit height to fit the text.

```qml
import Ljos.Ui
import Ljos.Layout
import Ljos.Controls.Basic

Pane {
    width: 240
    height: 120
    font.size: 20
    font.italic: true

    Column {
        spacing: 8

        Label {
            text: "Inherits 20 px and italic"
        }

        Button {
            text: "Also 20 px, and bold"
            font.weight: Font.Bold
        }

        Text {
            text: "A plain Text keeps its own font"
        }
    }
}
```

Here the `Label` and the `Button` inherit `20` px and italic from the `Pane`, and the `Button` adds `font.weight: Font.Bold`.

Keep a common pitfall in mind: a plain `Text` item does not inherit the `font` of the `Pane` or control it sits inside. Use `Label` when you want text to follow control font and palette inheritance.

## Limits

- There are no macOS, Windows or iOS styles; on Linux the platform style is Fusion.
- Fusion, Material and Universal draw Basic's shapes in each style's sizes and colours rather than each design system's full visuals. Material's `variant` and `shade()` are not available.
- `ToolTip` uses one shared instance per window. Popups play their `enter` and `exit` transitions (fading instead of moving under reduced motion), and a `Menu` or `ComboBox` popup with more rows than fit scrolls.
- In the Basic style, `palette.placeholderText` has a `3.04:1` contrast ratio on white; set `palette.placeholderText` yourself if your application needs WCAG AA contrast. `Ljos.Controls` meets 4.5:1 for its `placeholder` role.
- A plain `Item` gives child controls the active style's default colours when nothing above it sets a colour; the desktop's palette is not read.

## See also

- [Buttons and Toggles](./buttons-and-toggles.md)
- [Value Inputs](./value-inputs.md)
- [Containers and Popups](./containers-and-popups.md)
- [Styles](./styles.md)
- [Windows](./windows.md)
- [Accessibility](./accessibility.md)
- [Localisation](./localisation.md)
