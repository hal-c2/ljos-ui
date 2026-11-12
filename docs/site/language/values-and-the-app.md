---
title: Colours, the app and enums
description: Make colours, points and other values, reach the running application and its locale, and name the keys of alignment, keyboard and pointer enums in a file of Ljós's API.
section: language
order: 8
---

# Colours, the app and enums

After reading this page, you will be able to make and shade colours, build points, sizes and other values, read and change the running application through `App`, format dates for a locale, and name the keys of alignment, keyboard and pointer enums. Everything here comes from `Ljos.Core`, apart from fonts and enums, which come from `Ljos.Ui`.

## Colours

`rgba`, `hsla` and `hsva` make a colour from its components, each between `0` and `1`. A colour's name (`"red"`, `"#336699"`) goes wherever a `color` is expected. `Color.of(c)` takes a colour or a colour's name and gives a colour with methods: `darker(f)`, `lighter(f)`, `tint(t)`, `alpha(a)` and `equals(other)`. A property already typed `color` has those methods itself.

```qml
import Ljos.Ui
import Ljos.Core

Rectangle {
    width: 240
    height: 120
    property color base: rgba(0.2, 0.4, 0.6, 1)
    property bool sameRed: Color.of("red").equals("#ff0000")
    fill: base.lighter(1.4)

    Rectangle {
        anchors.centerIn: parent
        width: 120
        height: 60
        fill: Color.of("seagreen").darker(1.5)
    }
}
```

## Points, sizes and other values

`point(x, y)`, `size(w, h)`, `rect(x, y, w, h)`, `vector2d`, `vector3d`, `vector4d`, `quaternion` and `matrix4x4` make the value of their name. `Font.of({ … })` makes a font from its fields, and `Font.families()` lists the families the system has.

```qml
import Ljos.Ui
import Ljos.Core

Item {
    width: 240
    height: 120
    property point spot: point(10, 20)
    property size box: size(80, 40)
    property real m11: matrix4x4(2, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1).m11
    property int px: Font.of({ family: "Sans", pixelSize: 20 }).pixelSize
}
```

## The application

`App` is the running application:

- `App.name`, `App.version`, `App.organization`, `App.domain` and `App.displayName` describe it, and `App.arguments` lists what it was started with.
- `App.platform.os` names the system it runs on.
- `App.language` is the language its text is translated into. It is writable, and the text that depends on it follows when it changes.
- `App.hints` holds the system's hints for input and motion, such as `App.hints.mousePressAndHoldInterval`, and `App.inputMethod` the on-screen keyboard, such as `App.inputMethod.visible`.
- `App.openUrlExternally(u)` opens a link in the system's browser, and `App.quit()` ends the application. `App.exit(code)` ends it too; a code other than `0` does not reach the process yet, and the log says so.

Three functions go with it. `later(f, …)` calls `f` with the rest of its arguments once the current event has been handled. `url(u)` resolves `u` against the file it is written in. `bind(f)`, assigned to a property, makes the property follow `f` again after an assignment has broken its binding.

```qml
import Ljos.Ui
import Ljos.Core

Item {
    width: 240
    height: 120
    property string named: App.name
    property bool typing: App.inputMethod.visible
    property int base: 3
    property int twice: 0
    property int calls: 0

    function tick(): void {
        later(() => calls += 1)
    }

    Component.onCompleted: {
        twice = bind(() => base * 2)
        App.language = "is_IS"
    }
}
```

## Locales and dates

`Locale.current` is the locale the application runs in, and `Locale.named("de_DE")` is the locale of that name. A locale formats a date or a time with `formatDate`, `formatTime` and `formatDateTime`, given a format such as `"yyyy-MM-dd"`.

```qml
import Ljos.Ui
import Ljos.Core

Item {
    width: 240
    height: 120
    property string when: "2020-01-02T12:00"
    property string day: Locale.current.formatDate(when, "yyyy-MM-dd")
    property string german: Locale.named("de_DE").name
}
```

## Enums

An alignment, a key, a pointer button, a modifier, a cursor and the other keys a program passes around each belong to a type named for what they are, so a key says what it is: `Align.Left`, `Key.Return`, `MouseButton.Left`, `Modifier.Control`, `Orientation.Horizontal`, `Elide.Right`, `Cursor.PointingHand`, `CheckState.Checked`, `Direction.RightToLeft`. A key that would start with a digit is spelled out: `Key.Digit0`.

`Align`, `Modifier` and `MouseButton` are sets of flags, combined with `|`. The other enums are plain: a value is one of their keys.

```qml
import Ljos.Ui
import Ljos.Core

Item {
    width: 240
    height: 120
    property int corner: Align.Left | Align.Top
    property int presses: 0

    function press(key: int): void {
        if (key === Key.Digit0) presses += 1
    }
}
```

## See also

- [Properties and bindings](properties-and-bindings.md)
- [Signals and handlers](signals-and-handlers.md)
- [Bringing existing QML](porting-from-qt.md), for where each member of a file written for Qt goes
