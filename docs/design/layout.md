# Layout

A layout is right when it holds at every window size and font scale a user can choose, not only at the one it was drawn at. The audit lays the scene out at the window's size and at a narrow and a short window (`limits::audit::NARROW_FRACTION`, `limits::audit::SHORT_FRACTION` of it), at each font scale, and measures what moved, overflowed or covered something.

## Principles

- **Let layouts place things.** `RowLayout`, `ColumnLayout` and `GridLayout` know each item's implicit and minimum size; anchors and fixed `x`/`y` do not, and stop working when text grows.
- **Sizes come from content.** A popup, card or dialog takes `implicitWidth`/`implicitHeight` from what it holds, within a maximum, and scrolls past it.
- **One spacing scale.** Every margin, padding and spacing is a multiple of `limits::SPACING_UNIT_PX` (Material's 4 dp grid): 4, 8, 12, 16, 24, 32. Related things sit closer than unrelated ones (proximity).
- **Decide what gives way.** At the smallest window, write down what wraps, what elides, what scrolls and what moves into an overflow menu. Nothing operable may leave the window.
- **Nested corners.** A child's radius is the parent's radius less the padding between them, never more.

## Rules

<a id="layout-offscreen"></a>
### `layout-offscreen`

**Error, runtime.** Something a user can operate lies partly or wholly outside the window, at some window size or font scale, and nothing scrolls it into view. A Save button below the bottom edge of a dialog does not exist for the user.

```qml
// Bad: a fixed column taller than a short window
Column { Repeater { model: 12; CheckBox { text: qsTr("Option %1").arg(index + 1) } } Button { text: qsTr("Save") } }
// Good: the options scroll, the button stays
ColumnLayout {
    anchors.fill: parent
    ScrollView { Layout.fillWidth: true; Layout.fillHeight: true
        Column { Repeater { model: 12; CheckBox { text: qsTr("Option %1").arg(index + 1) } } } }
    Button { text: qsTr("Save"); Layout.alignment: Qt.AlignRight }
}
```

Sources: https://www.w3.org/WAI/WCAG22/Understanding/reflow.html , https://developer.apple.com/design/human-interface-guidelines/layout

<a id="layout-occluded"></a>
### `layout-occluded`

**Error, runtime.** Something a user can operate is covered by another item drawn above it: a toast over the send button, a banner over the first row. The click lands on the cover.

```qml
// Bad: a notice anchored over the footer's buttons
Label { anchors.bottom: parent.bottom; text: qsTr("Copied") }
// Good: the notice takes its own row in the layout
ColumnLayout { anchors.fill: parent
    ListView { Layout.fillWidth: true; Layout.fillHeight: true }
    Label { visible: copied; text: qsTr("Copied") }
    RowLayout { Button { text: qsTr("Send") } } }
```

Sources: https://www.w3.org/WAI/WCAG22/Understanding/focus-not-obscured-minimum.html

<a id="layout-overlap"></a>
### `layout-overlap`

**Warning, runtime.** Two pieces of text or two controls that are not meant to stack are drawn over each other, usually because anchors ignore each other's size once text grows.

```qml
// Bad
Item { Label { text: model.title } Label { anchors.right: parent.right; text: model.date } }
// Good
RowLayout { Label { text: model.title; elide: Text.ElideRight; Layout.fillWidth: true } Label { text: model.date } }
```

Checked by: the runtime audit, on two shown texts or two controls of the application whose drawn boxes cross by more than a pixel, neither inside the other. Scroll bars, which meet in a corner by design, are left out. Controls in two cells of one layout or positioner are compared too, by what each cell shows: a control that crosses into its neighbour's cell (a `Flow` of buttons narrower than they are, beside a Send button in a `RowLayout`) has outgrown its own.

Sources: https://www.w3.org/WAI/WCAG22/Understanding/text-spacing.html

<a id="layout-under-minimum"></a>
### `layout-under-minimum`

**Warning, runtime.** An item is laid out smaller than its own minimum or implicit size, so its content squeezes or slides under its neighbour. The window's minimum size does not add up to its parts.

```qml
// Bad
ApplicationWindow { minimumWidth: 200; RowLayout { anchors.fill: parent; Pane { Layout.minimumWidth: 240 } Pane { Layout.fillWidth: true } } }
// Good
ApplicationWindow { minimumWidth: 480; RowLayout { anchors.fill: parent; Pane { Layout.minimumWidth: 240 } Pane { Layout.fillWidth: true; Layout.minimumWidth: 200 } } }
```

Sources: https://developer.apple.com/design/human-interface-guidelines/layout

<a id="layout-fill-crushed"></a>
### `layout-fill-crushed`

**Warning, runtime.** An item that fills a row (or column) of a layout is laid out smaller than what it holds needs, while a sibling that does not fill keeps its preferred size and could give the room. A `Flow`, a plain `Item` or a `Text` asks the layout for no least size, so a side panel's `Layout.preferredWidth` wins and the main column is crushed: its buttons squeeze, wrap and overlap far from the cause. What an item needs is counted as a floor: a button's implicit size, a `Layout.minimum` size, and what rows and columns add up from those.

```qml
// Bad
RowLayout { anchors.fill: parent
    ColumnLayout { Layout.fillWidth: true; Flow { Layout.fillWidth: true; Button { text: qsTr("Model") } } }
    Pane { Layout.preferredWidth: 540 } }
// Good
RowLayout { anchors.fill: parent
    ColumnLayout { Layout.fillWidth: true; Layout.minimumWidth: 280; Flow { Layout.fillWidth: true; Button { text: qsTr("Model") } } }
    Pane { Layout.preferredWidth: 540; Layout.maximumWidth: parent.width / 2 } }
```

Sources: https://doc.qt.io/qt-6/qml-qtquick-layouts-layout.html, https://developer.apple.com/design/human-interface-guidelines/layout

<a id="layout-popup-fit"></a>
### `layout-popup-fit`

**Warning, runtime.** A popup or dialog is much larger than what it holds (its content fills less than `limits::POPUP_MIN_FILL` of it) or too small to show it. A huge empty dialog reads as unfinished; a cramped one hides its buttons.

```qml
// Bad
Dialog { width: 600; height: 500; Label { text: qsTr("Delete this file?") } }
// Good
Dialog { title: qsTr("Delete this file?"); standardButtons: Dialog.Cancel | Dialog.Ok
    Label { text: qsTr("It moves to the trash.") } }
```

Sources: https://m3.material.io/components/dialogs/guidelines

<a id="layout-fixed-size"></a>
### `layout-fixed-size`

**Warning, static.** A popup, dialog or pane with content that changes has a literal width and height. It fits English at 100 % and nothing else.

```qml
// Bad
Popup { width: 240; height: 120; contentItem: Label { text: message } }
// Good
Popup { width: Math.min(implicitWidth, 400); contentItem: Label { text: message; wrapMode: Text.Wrap } }
```

Sources: https://www.w3.org/WAI/WCAG22/Understanding/resize-text.html

<a id="layout-misaligned"></a>
### `layout-misaligned`

**Hint, runtime.** Controls in one column start a few pixels apart (up to `limits::MISALIGN_MAX_PX`; further apart is taken as deliberate), or fields of one kind in a form have different widths. The eye reads near-misses as mistakes.

```qml
// Bad
Column { TextField { x: 2 } TextField { x: 6; width: 180 } }
// Good
GridLayout { columns: 2
    Label { text: qsTr("Name") }  TextField { Layout.fillWidth: true }
    Label { text: qsTr("Email") } TextField { Layout.fillWidth: true } }
```

Checked by: the runtime audit, on controls stacked in one column whose left edges differ by 1 to `limits::MISALIGN_MAX_PX`, on fields of one kind stacked in one column where most share a width and some do not, and on a menu where some items have an icon and others not.

Sources: https://www.refactoringui.com/ , https://m3.material.io/foundations/layout/understanding-layout/spacing

<a id="layout-anchors-in-layout"></a>
### `layout-anchors-in-layout`

**Error, static.** An item directly inside a layout sets anchors, which the layout overrides; what the author meant never happens and Qt warns at run time.

```qml
// Bad
RowLayout { Button { anchors.right: parent.right; text: qsTr("Next") } }
// Good
RowLayout { Item { Layout.fillWidth: true } Button { text: qsTr("Next") } }
```

Sources: https://doc.qt.io/qt-6/qtquicklayouts-overview.html

<a id="layout-spacing-scale"></a>
### `layout-spacing-scale`

**Hint, static.** A spacing, margin or padding is not a multiple of `limits::SPACING_UNIT_PX`. Off-scale gaps (7, 13, 19) make rhythm uneven.

```qml
// Bad
ColumnLayout { spacing: 7 }
// Good
ColumnLayout { spacing: 8 }
```

Sources: https://m3.material.io/foundations/layout/understanding-layout/spacing , https://www.refactoringui.com/

<a id="layout-shift"></a>
### `layout-shift`

**Warning, runtime.** Content moves by more than `limits::LAYOUT_SHIFT_MIN_PX` after it is shown: a load, a click or a new status line pushes what is under the pointer, and the next click lands on the wrong thing.

```qml
// Bad: the hint appears and pushes the field down
ColumnLayout { Label { visible: error !== ""; text: error } TextField {} }
// Good: the space is always there
ColumnLayout { TextField {} Label { text: error; opacity: error !== "" ? 1 : 0 } }
```

Checked by: the runtime audit, which looks twice: when the scene first comes to rest, and after half a second more of application time. A control or a text that came, went, moved or changed size by `limits::LAYOUT_SHIFT_MIN_PX` or more is reported at what moved it (an earlier sibling in a row, column, flow, grid or layout that changed) or else at the outermost thing that moved with it. Popups, and what a view scrolled, are left out; so is every point of the matrix but the scene as it is, where the first look was taken. What starts to move within `limits::LAYOUT_SHIFT_INPUT_GRACE_MS` of the user's input is that input's doing, as in CLS: a test host takes the first look again once that much time has passed since the input.

Sources: https://web.dev/articles/cls , https://vercel.com/design/guidelines

<a id="layout-stacked-scrims"></a>
### `layout-stacked-scrims`

**Warning, runtime.** Two dimming overlays are open at once over the same content, so the window goes nearly black and it is unclear which layer is active.

```qml
// Bad: a dialog opened from a modal drawer, both dim
Drawer { modal: true; Button { onClicked: confirm.open() } }
Dialog { id: confirm; modal: true }
// Good: the drawer closes first, or only the top one dims
Drawer { id: drawer; modal: true; Button { onClicked: { drawer.close(); confirm.open() } } }
Dialog { id: confirm; modal: true }
```

Checked by: the runtime audit, on two or more open popups that each dim the window (`dim`, which follows `modal`) at once; the second and later are reported.

Sources: https://m3.material.io/components/dialogs/guidelines

<a id="layout-wrap-separator"></a>
### `layout-wrap-separator`

**Hint, runtime.** A wrapping row (a `Flow` of tags or breadcrumbs) begins or ends a line with a separator, a dangling `·` or `|`.

```qml
// Bad: separators are items of their own
Flow { Label { text: "Draft" } Label { text: "·" } Label { text: "3 files" } }
// Good: each group carries its separator, and the first carries none
Flow { Repeater { model: parts; Label { text: (index > 0 ? "· " : "") + modelData } } }
```

Checked by: the runtime audit, on a `Flow` that has wrapped onto more than one line and starts or ends a line with a `ToolSeparator`, a `MenuSeparator` or a thin line.

Sources: https://www.refactoringui.com/

<a id="layout-menu-ungrouped"></a>
### `layout-menu-ungrouped`

**Hint, static and runtime.** A menu holds more than `limits::MENU_MAX_UNGROUPED` items and no separator or submenu, so people read the whole list every time.

```qml
// Bad: twelve items in one run
Menu { MenuItem { text: qsTr("Cut") } MenuItem { text: qsTr("Copy") } /* … ten more … */ }
// Good
Menu {
    MenuItem { text: qsTr("Cut") } MenuItem { text: qsTr("Copy") } MenuItem { text: qsTr("Paste") }
    MenuSeparator {}
    Menu { title: qsTr("Share") MenuItem { text: qsTr("Copy link") } }
}
```

Checked by: the runtime audit as well, on a menu built at run time (a `Repeater` or `Instantiator` of items) with more than `limits::MENU_MAX_UNGROUPED` items and no separator or submenu; it is reported on the menu.

Sources: https://developer.apple.com/design/human-interface-guidelines/menus , https://www.nngroup.com/articles/menu-design/

<a id="layout-resize-handle"></a>
### `layout-resize-handle`

**Warning, runtime.** An edge the user can drag to resize (a split handle, a panel edge) draws nothing and does not change on hover, so nobody finds it.

```qml
// Bad
SplitView { handle: Item { implicitWidth: 6 } }
// Good
SplitView {
    handle: Rectangle {
        implicitWidth: 6
        color: SplitHandle.pressed ? palette.highlight : SplitHandle.hovered ? palette.mid : palette.midlight
    }
}
```

Checked by: the runtime audit, on a `SplitView` handle that, at rest, draws nothing in itself or anything it holds. What it draws on hover is not looked at, and neither are the resize edges of a frameless window.

Sources: https://www.w3.org/WAI/WCAG22/Understanding/non-text-contrast.html , https://developer.apple.com/design/human-interface-guidelines/split-views

<a id="layout-window-controls"></a>
### `layout-window-controls`

**Warning, runtime.** A frameless window shows no close control, or its window controls are not at the top corner where the platform puts them, so the window cannot be closed or moved the usual way.

```qml
// Bad
ApplicationWindow { flags: Qt.Window | Qt.FramelessWindowHint }
// Good
ApplicationWindow {
    flags: Qt.Window | Qt.FramelessWindowHint
    header: ToolBar { RowLayout { anchors.fill: parent
        Label { text: title; Layout.fillWidth: true }
        ToolButton { text: qsTr("Close"); display: AbstractButton.IconOnly; icon.name: "window-close"; onClicked: close() } } }
}
```

Checked by: the runtime audit, on a window with `Qt.FramelessWindowHint` that shows no close control (told by its name or icon: close, quit, exit), or whose close control is not the outermost control at the top right or top left within `limits::WINDOW_CONTROLS_MAX_INSET_PX`. What a popup holds is never the window's.

Sources: https://developer.apple.com/design/human-interface-guidelines/windows , https://learn.microsoft.com/en-us/windows/apps/design/basics/titlebar-design

<a id="layout-nested-radius"></a>
### `layout-nested-radius`

**Hint, static.** A `Rectangle` with a literal `radius` fills (`anchors.fill: parent`) another `Rectangle` with a smaller literal `radius`. The inner corners then stick out of, or leave a wedge against, the outer ones. The inner radius is the outer radius less the gap between them, and never below 0. Only numbers written out are compared.

```qml
// Bad: an 8 px card in a 4 px frame
Rectangle { radius: 4
    Rectangle { anchors.fill: parent; anchors.margins: 2; radius: 8 } }
// Good: 4 - 2 = 2
Rectangle { radius: 4
    Rectangle { anchors.fill: parent; anchors.margins: 2; radius: 2 } }
```

Sources: https://www.refactoringui.com/ , https://developer.apple.com/design/human-interface-guidelines/layout

<a id="layout-reflow"></a>
### `layout-reflow`

**Warning, runtime.** At `limits::REFLOW_WIDTH_PX` wide (320 logical px, WCAG's 320 CSS px), a view of text is wider and taller than itself, so every line is read by scrolling sideways and back. A table, a map, a picture or a canvas scrolls both ways by its nature and is not this.

```qml
// Bad: the text keeps its own width however narrow the window
ScrollView { Label { text: article; width: 720; wrapMode: Text.Wrap } }
// Good: the text takes the view's width and wraps; it scrolls one way
ScrollView { id: view; Label { text: article; width: view.availableWidth; wrapMode: Text.Wrap } }
```

Checked by: the runtime audit, at a point of the matrix where the window is 320 px wide or narrower (its minimum width, or a size asked for): the outermost `Flickable` (a `ScrollView`'s too) that holds text and is no table, whose content is wider and taller than the view.

Sources: https://www.w3.org/WAI/WCAG22/Understanding/reflow.html

## Not checked yet

- **Design every density**: empty, one row, a screenful and a thousand rows, and the error state. https://vercel.com/design/guidelines

## Sources

- WCAG 2.2 reflow, resize text, focus not obscured: https://www.w3.org/WAI/WCAG22/Understanding/reflow.html , https://www.w3.org/WAI/WCAG22/Understanding/resize-text.html , https://www.w3.org/WAI/WCAG22/Understanding/focus-not-obscured-minimum.html
- Material 3 spacing: https://m3.material.io/foundations/layout/understanding-layout/spacing
- Apple HIG layout and windows: https://developer.apple.com/design/human-interface-guidelines/layout , https://developer.apple.com/design/human-interface-guidelines/windows
- Qt Quick Layouts overview (documentation): https://doc.qt.io/qt-6/qtquicklayouts-overview.html
- web.dev, cumulative layout shift: https://web.dev/articles/cls
- Vercel web interface guidelines: https://vercel.com/design/guidelines ; Refactoring UI: https://www.refactoringui.com/
