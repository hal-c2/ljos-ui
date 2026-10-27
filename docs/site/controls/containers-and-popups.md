---
title: Containers and Popups
description: Group and navigate content with Pane, Frame, GroupBox, Page, ScrollView, TabBar, SwipeView, StackView, Popup, Dialog, Drawer, Menu, and ToolTip.
section: controls
order: 4
---
# Containers and Popups

Use container controls to group items, add headers or scroll bars, and switch between pages; use popups, dialogs, drawers, menus, and tool tips to show transient surfaces above the window.

## `Pane`, `Frame`, `GroupBox`, `Page`, and `ScrollView`

`Pane` provides a padded surface with a `#ffffff` background in the Basic style. When empty, its implicit size equals its padding (`24 × 24` from `padding: 12`), and any items declared inside it become children of its `contentItem` (`contentWidth`, `contentHeight`).

`Frame` is also as large as its padding when empty, with a transparent background and a `1` px border (`#bdbdbd`) that follows `palette.mid`. `GroupBox` builds on `Frame`: without a `title`, it draws like a plain frame; when `title` is set, it displays the title at `x: 12` in a replaceable `label` item and starts the frame below the title.

Write this as `Main.qml` and run `ljos check` on its directory:

```qml
import QtQuick
import QtQuick.Controls.Basic

Column {
    padding: 12
    spacing: 12

    Pane {
        padding: 12

        Label {
            text: "A pane around its content"
        }
    }

    Frame {
        Label {
            text: "A frame draws a line round its content"
        }
    }

    GroupBox {
        title: "Options"

        Column {
            spacing: 6

            CheckBox {
                text: "Show hidden files"
            }
        }
    }
}
```

`Page` places its `header` at the top and its `footer` at the bottom, both stretched to the page's width, with `contentItem` between them inside the padding and spacing (plus a `title` property). An empty `Page` has size `0 × 0`, and hiding `header` gives its vertical space back to the content.

`ScrollView` has size `0 × 0` when empty. When given one child item, its internal `Flickable` fills the view inside the padding. `ScrollBar.vertical` runs down the right edge and `ScrollBar.horizontal` runs along the bottom edge, each sized to the share of content currently in view; switching a scroll bar off via its attached policy removes it so it takes no room.

## `Popup`, `Dialog`, and `Drawer`

`Popup` is the base for transient overlays. It starts closed (`visible: false`), sizes itself to its content plus padding, and positions itself relative to the top-left corner of the parent item that declared it, moving if that parent moves. Calling `open()` or setting `visible: true` emits `aboutToShow` and then `opened`; calling `close()` or setting `visible: false` emits `aboutToHide` and then `closed` (calling `open()` when already open or `close()` when already closed does nothing). A popup with an `enter` or `exit` transition is `opened` when its enter transition has run and `closed` when its exit transition has finished; with reduced motion, a transition that would move the popup fades it instead.

By default (`closePolicy`), a non-modal popup closes when the user presses outside it or presses Escape. If two popups are stacked, pressing on the lower popup closes only the top one; pressing beside a `modal: true` popup does not close a non-modal popup underneath it.

`Dialog` adds a bold `title` header and a `standardButtons` footer (such as `Dialog.Ok` and `Dialog.Cancel`), starting with `title: ""`, `standardButtons: 0`, and `result: 0`. If a dialog has neither a title nor buttons, neither bar is shown; when closed, neither bar contributes to its size. Calling `accept()` sets `result` to `1`, emits `accepted`, closes the dialog, and emits `closed`; `reject()` does the same with `rejected`, and `done(result)` closes the dialog with any integer `result` you pass.

```qml
import QtQuick
import QtQuick.Controls.Basic

Item {
    width: 300
    height: 200

    Dialog {
        id: confirm
        title: "Delete the file?"
        standardButtons: Dialog.Ok | Dialog.Cancel
        modal: true

        Label {
            text: "This cannot be undone."
        }

        onAccepted: result.text = "Deleted"
        onRejected: result.text = "Kept"
    }

    Button {
        text: "Delete"
        onClicked: confirm.open()
    }

    Text {
        id: result
        y: 60
    }
}
```

`Drawer` is a popup that slides in from a window edge (`edge`: `Qt.TopEdge` `1`, `Qt.LeftEdge` `2`, `Qt.RightEdge` `4`, or `Qt.BottomEdge` `8`). Its `position` runs from `0` (hidden) to `1` (fully open), and it is modal by default, dimming the window behind it unless `modal` is false. When `interactive` is true, dragging from within `dragMargin` of the edge pulls the drawer in: releasing a closed drawer before `position` reaches `0.3` snaps it shut, releasing at or past `0.3` opens it, and releasing an open drawer above `0.7` keeps it open.

## `TabBar`, `SwipeView`, and `StackView`

`TabBar` divides its width evenly among its `TabButton` children (which are checkable and auto-exclusive), starts with the first button current (`currentIndex: 0`), and updates `currentIndex` whenever a tab button is clicked or checked.

`SwipeView` lays out its pages side by side (horizontal and `interactive: true` by default, with configurable `orientation`), sizing each page to the view. Setting `currentIndex` or swiping past the threshold moves to the target page, whereas a short swipe springs back. Binding `currentIndex` between a `TabBar` and a `SwipeView` keeps them in step:

```qml
import QtQuick
import QtQuick.Controls.Basic

Column {
    width: 300
    spacing: 0

    TabBar {
        id: bar
        width: parent.width
        currentIndex: swipe.currentIndex

        TabButton {
            text: "Inbox"
        }
        TabButton {
            text: "Sent"
        }
    }

    SwipeView {
        id: swipe
        width: parent.width
        height: 120
        currentIndex: bar.currentIndex

        Pane {
            Label {
                text: "Inbox page"
            }
        }
        Pane {
            Label {
                text: "Sent page"
            }
        }
    }
}
```

`StackView` manages a stack of pages (`depth`), starting on `initialItem`. You navigate with `push()`, `pop()`, `replace()`, and `clear()`. When you `push()` a page, the incoming page is activated before the outgoing page is deactivated, and pushing multiple pages at once instantiates them lazily as needed. When you `pop()` a page, the leaving page is deactivated first and destroyed if the stack created it from a component; calling `pop()` when only one page remains leaves that last page in place.

## `Menu`, `MenuBar`, and `ToolTip`

`Menu` starts with `count: 0` and collects declared `MenuItem` or `MenuSeparator` children in order. You can also modify it at run time with `addItem()`, `moveItem()`, and `takeItem()` (which removes and returns the item). Moving the pointer over an enabled `MenuItem` makes it current (disabled items and separators never become current), and resting the pointer on a sub-menu item opens the sub-menu after a brief pause. A `Menu` with more items than fit in the window is no taller than the window and scrolls. Right-clicking an item that has an attached `ContextMenu.menu` requests that menu at the click position.

`MenuBar` creates a `MenuBarItem` for each `Menu` added to it (`addMenu()`, `removeMenu()`, `count`), matching the height of its items and opening a menu as soon as the pointer presses its `MenuBarItem` (Escape or pressing outside closes it).

```qml
import QtQuick
import QtQuick.Controls.Basic

Item {
    width: 300
    height: 200

    Menu {
        id: fileMenu

        MenuItem {
            text: "New"
            onTriggered: status.text = "New chosen"
        }
        MenuItem {
            text: "Open"
            onTriggered: status.text = "Open chosen"
        }
    }

    Button {
        text: "File"
        onClicked: fileMenu.open()
    }

    Text {
        id: status
        y: 60
    }
}
```

`ToolTip` displays short help text centered above its parent. Calling `show()` sets `text` and opens the tool tip; if `delay` is set, it waits before appearing (calling `hide()` during the delay cancels it), and if `timeout` is set, it closes automatically once the timeout expires.

## Differences from Qt

- `ToolTip` uses a single shared instance per window, and the attached `ToolTip` property reports that shared state while waiting for its `delay`.
- When a popup closes, its item is hidden inside the window overlay rather than removed from the overlay tree.

## See also

- [Quick Controls Overview](./overview.md)
- [Windows](./windows.md)
- [Accessibility](./accessibility.md)
