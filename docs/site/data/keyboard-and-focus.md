---
title: Keyboard Input, Shortcuts, and Focus
description: Manage activeFocus, FocusScope, the Tab chain, directional KeyNavigation, Keys event bubbling, and window-wide Shortcut sequences.
section: data
order: 6
---
# Keyboard Input, Shortcuts, and Focus

Keyboard input in `QtQuick` flows to the item with `activeFocus`. You control which item receives key events using `focus`, `FocusScope`, `activeFocusOnTab`, and `KeyNavigation`, handle raw key presses and releases with the attached `Keys` object, and bind chord sequences across the window with `Shortcut`.

## Focus, FocusScope, and the Tab order

Every `Item` has `focus`, `activeFocus`, `activeFocusOnTab`, `focusPolicy`, `forceActiveFocus()`, and `nextItemInFocusChain()`. You can inspect which item currently holds active focus in the window via `Window.activeFocusItem` (which is `null` if no item has active focus or when the window is in the background).

- At startup, the first item that sets `focus: true` receives both `focus` and `activeFocus`. Setting `focus: true` on another sibling takes focus from the first, notifying only the two items that lost and gained `activeFocus`.
- Calling `forceActiveFocus()` (optionally with a focus reason) gives an item `focus` and `activeFocus`, and also gives `focus` to any enclosing `FocusScope`. Clicking an item with the pointer does not move focus automatically.
- Inside a `FocusScope` that does not itself have `focus`, a child item with `focus: true` has `focus` set, but its `activeFocus` is `false`. When the `FocusScope` gains `focus`, that child immediately receives `activeFocus`; when focus leaves the `FocusScope`, the child remembers `focus: true` for the next time the scope is focused.

```qml
import QtQuick

Item {
    width: 200
    height: 200
    property string active: Window.activeFocusItem ? Window.activeFocusItem.objectName : ""
    Item { id: one; objectName: "one"; activeFocusOnTab: true; focus: true }
    FocusScope {
        objectName: "scope"
        Item { objectName: "inside"; activeFocusOnTab: true; focus: true }
    }
    Item { objectName: "three"; activeFocusOnTab: true }
}
```

Pressing `Tab` moves focus forward through items that have `activeFocusOnTab: true` (including nested items) and wraps from the last item back to the first; `Shift+Tab` moves backwards and wraps to the last item. `Tab` skips hidden items, disabled items, and items without `activeFocusOnTab`. You can query the next or previous item in the chain with `nextItemInFocusChain()`.

Watch out for how visibility, enabling, and window activation affect focus:

- Hiding (`visible: false`) an item that already has `activeFocus` does not take `activeFocus` away from it.
- Disabling (`enabled: false`) an item or its enclosing `FocusScope` clears `activeFocus` while keeping `focus: true`; re-enabling it restores `activeFocus`. Destroying the focused item leaves `Window.activeFocusItem` as `null`.
- When the window moves to the background, no item has `activeFocus` and neither keys nor shortcuts are delivered; `activeFocus` returns when the window becomes active again.

## Directional focus with KeyNavigation

The attached `KeyNavigation` properties (`KeyNavigation.right`, `left`, `up`, `down`, `tab`, and `backtab`) move focus to the target item when the corresponding arrow key, `Tab`, or `Shift+Tab` is pressed.

```qml
import QtQuick

Row {
    Rectangle {
        id: a
        width: 50; height: 50
        color: "#fc9"
        focus: true
        KeyNavigation.right: b
        KeyNavigation.tab: c
    }
    Rectangle {
        id: b
        width: 50; height: 50
        color: "#cde"
        KeyNavigation.right: c
        KeyNavigation.backtab: a
    }
    Rectangle {
        id: c
        width: 50; height: 50
        color: "#dfd"
    }
}
```

Pressing `Right` moves focus from `a` to `b` and then to `c`. Pressing `Tab` on `a` jumps to `c`, and `Shift+Tab` on `b` returns to `a`.

- A direction with no item assigned does nothing, and a target item that cannot take focus is skipped.
- `KeyNavigation` runs after the item's own key handling by default: if a `Keys` handler on the item accepts the key press, focus stays where it is.

## Handling key events with Keys

A key press or release starts at the item with `activeFocus` and bubbles up through its parent items until a handler accepts the event (`event.accepted = true`). If no item in the window has `activeFocus`, key presses reach no `Keys` handlers at all.

The attached `Keys` object provides specific key signals (which run before the general `Keys.onPressed` handler), `Keys.onPressed`, `Keys.onReleased`, `Keys.onShortcutOverride`, `Keys.enabled`, `Keys.priority` (`Keys.BeforeItem` by default, or `Keys.AfterItem`), and `Keys.forwardTo`.

```qml
import QtQuick

Item {
    id: root
    width: 200
    height: 100
    property string log: ""
    Keys.onPressed: (event) => log += "root:" + event.key + " "
    Item {
        Keys.onPressed: (event) => root.log += "outer "
        Item {
            focus: true
            Keys.onPressed: (event) => {
                root.log += "inner "
                event.accepted = false
            }
        }
    }
}
```

Here the innermost item has `focus` and logs `"inner "`, then sets `event.accepted = false` so the event bubbles to `"outer "` and finally to `"root:..."`.

- Setting `Keys.enabled: false` skips all `Keys` handlers on that item.
- `Keys.forwardTo` forwards the key event to the listed items in order before normal handling; if any forwarded item accepts the press, processing stops there.

## Window shortcuts with Shortcut

Every key press goes through three dispatch phases in order: `ShortcutOverride` on the focus chain, then matching `Shortcut` items, and finally `Keys` handlers bubbling up from the focused item.

A `Shortcut` matches a single `sequence` (such as `"Ctrl+S"` or a platform `StandardKey` like `StandardKey.Copy`) or a list of `sequences`, and emits `activated` when triggered. Unlike `Keys`, a `Shortcut` works even when no item has focus or when the `Shortcut` is declared inside a hidden or disabled item.

```qml
import QtQuick

Item {
    id: root
    width: 200
    height: 100
    focus: true
    property string log: ""
    Keys.onShortcutOverride: (event) => event.accepted = false
    Shortcut {
        sequence: "Ctrl+S"
        onActivated: root.log += "save "
    }
    Shortcut {
        sequences: ["Ctrl+F", "F3"]
        onActivated: root.log += "find "
    }
    Shortcut {
        sequence: StandardKey.Copy
        onActivated: root.log += "copy "
    }
}
```

Keep these rules in mind when defining shortcuts:

- The focused item can claim a key before any `Shortcut` sees it by setting `event.accepted = true` in `Keys.onShortcutOverride`.
- When a `Shortcut` takes a key press, the subsequent key release still reaches `Keys.onReleased`. A disabled `Shortcut` (`enabled: false`) leaves the key press to `Keys`.
- Holding a key down triggers `activated` on each auto-repeat unless you set `autoRepeat: false`.
- Multi-chord sequences (two chords) activate on the second chord. Pressing a wrong second chord is swallowed and resets the sequence, and a complete single-chord shortcut wins over a two-chord shortcut that starts with the same chord.
- If multiple enabled shortcuts match the same key, each press emits `activatedAmbiguously` on one of them in turn, starting with the last one declared; disabling the others means the remaining one emits `activated` instead.

## Differences from Qt

- On Linux, `StandardKey` chooses platform key bindings from the desktop environment via `XDG_CURRENT_DESKTOP` (falling back to `KDE_FULL_SESSION` and `DESKTOP_SESSION`), matching Qt's behaviour.

## See also

- [Pointer Input and Gestures](./pointer-input.md)
- [ListView and GridView](./list-and-grid-views.md)
- [PathView, TableView, TreeView, and Flickable](./other-views.md)
