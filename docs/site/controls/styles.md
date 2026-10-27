---
title: Styles
description: Select a built-in style at startup or in QML, configure Material and Universal attached properties, or replace control parts.
section: controls
order: 5
---
# Styles

Use styles to switch the look and metrics of your controls across the whole application or within individual QML files, or to customise specific controls by setting attached theme properties or replacing their visual parts.

## Selecting a style

Every control (`Button`, `SpinBox`, `ComboBox`, and the rest) is present in all four built-in style modules:

- `QtQuick.Controls.Basic`
- `QtQuick.Controls.Fusion`
- `QtQuick.Controls.Material`
- `QtQuick.Controls.Universal`

When your QML files import the unstyled `QtQuick.Controls` module, Ljós chooses the active style at startup in this order:

1. The `-style` command-line option
2. The `QT_QUICK_CONTROLS_STYLE` environment variable
3. The `[Controls] Style=` key in the configuration file that `QT_QUICK_CONTROLS_CONF` names (a `qtquickcontrols2.conf` sitting beside your files is not read on its own)
4. The platform style, which is Fusion on Linux

If a QML file imports a style module by name—such as `import QtQuick.Controls.Material`—it always uses that style regardless of the startup setting or platform default.

Each style brings its own default control sizes:

- **Fusion**: `Button` is `80 × 24` with `padding: 4`
- **Material**: `Button` is `64 × 52` and `TextField` is `120 × 56`
- **Universal**: `Button` is `32 × 32`

## Material and Universal attached properties

When you use the Material or Universal style, you can configure themes and colours on any item, window, or control using attached properties. Those properties propagate down the item tree to child controls, update derived colours automatically when the theme changes, and reset to their inherited value when assigned `undefined`.

For `Material`:

- `Material.theme`: `Material.Light` (`0`), `Material.Dark` (`1`), or `Material.System` (`2`), which follows the platform's colour scheme
- `Material.accent`, `Material.primary`, and `Material.foreground`: inherited down the item tree (including named colours such as `Material.Teal` `8` or hex colour strings)
- `Material.elevation`: stays local to the control it is set on and is not inherited by child controls

Write this as `Main.qml` and run `ljos check` on its directory:

```qml
import QtQuick
import QtQuick.Controls.Material

Column {
    padding: 12
    spacing: 12
    Material.theme: Material.Dark
    Material.accent: Material.Teal

    Button {
        text: "Accent button"
        highlighted: true
    }

    Switch {
        text: "Notifications"
    }
}
```

Both the `Button` and the `Switch` inherit the dark theme and teal accent attached to the enclosing `Column`. You can also set `Material.primary` and `Material.foreground` on a plain `Item` container:

```qml
import QtQuick
import QtQuick.Controls.Material

Item {
    width: 300
    height: 120
    Material.primary: "#123456"
    Material.foreground: "#234567"

    Column {
        padding: 12
        spacing: 8

        Label {
            text: "Takes the foreground colour"
        }

        Button {
            text: "Inherits the primary colour"
        }
    }
}
```

For `Universal`, `Universal.theme` accepts `Universal.Light`, `Universal.Dark` (`1`), or `Universal.System` (the platform's colour scheme), and `Universal.accent` sets the accent colour:

```qml
import QtQuick
import QtQuick.Controls.Universal

Column {
    padding: 12
    spacing: 12
    Universal.theme: Universal.Dark

    Button {
        text: "Universal button"
    }

    Pane {
        Label {
            text: "Dark theme inherited"
        }
    }
}
```

## Customising parts and writing a custom style

You can customise an individual control without writing a full style:

- **Palettes and fonts**: Colours and fonts set or bound on a control (including controls instantiated later by a `Loader`) propagate to the items inside it and update whenever a binding changes.
- **Replacing `background`, `contentItem`, or `padding`**: Assigning your own item to `background` or `contentItem` replaces that part of the active style. If you assign it when the control is declared, the style's default part is never created; if you swap a `contentItem` later, the previous item is hidden and its `parent` is cleared.

To build a custom style for your application, create a `qmldir` module containing QML files named after the controls you want to restyle—for example, a `Button.qml` whose root item is `T.Button` from `QtQuick.Templates`. When your custom style is selected, its `Button` replaces the one in `QtQuick.Controls`, and any control your module does not define is taken from its fallback style (which defaults to Basic). To name another fallback style, set the `QT_QUICK_CONTROLS_FALLBACK_STYLE` environment variable or `FallbackStyle=` in the `[Controls]` group of the configuration file.

## Differences from Qt

- Ljós has no macOS, Windows, or iOS style; the native macOS and Windows styles are deferred, and the platform style is Fusion on Linux.
- Material's `variant` and `shade()` are not supported, and the built-in styles render Basic's shapes using each style's metrics and colours rather than full Material or Fluent visual effects.

## See also

- [Quick Controls Overview](./overview.md)
- [Buttons and Toggles](./buttons-and-toggles.md)
