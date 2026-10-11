# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-menu.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-menuitem.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-menuseparator.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-action.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Menu
  A menu is a popup with a column of items to choose from. An item may be
  checkable, stand for an action, or open another menu.

  Rule: What a menu is made of

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          id: root
          objectName: "root"
          width: 400; height: 400
          property string seen: ""
          function look() {
            var s = []
            for (var i = 0; i < m.count; i++) s.push(m.itemAt(i).text === undefined ? "-" : m.itemAt(i).text)
            seen = s.join(",") + " | " + (m.actionAt(3) === act) + (m.menuAt(4) === sub) + " " + m.actionAt(0)
              + m.menuAt(0) + m.itemAt(9) + " | " + (one.menu === m) + (m.itemAt(4).subMenu === sub)
              + (sub.parent === m.itemAt(4)) + (m.itemAt(3).action === act)
          }
          Menu { objectName: "empty" }
          Menu {
            id: m; objectName: "m"; title: "Top"
            MenuItem { id: one; objectName: "one"; text: "One" }
            MenuItem { objectName: "two"; text: "Two"; checkable: true }
            MenuSeparator { objectName: "line" }
            Action { id: act; text: "Act" }
            Menu { id: sub; objectName: "sub"; title: "Sub"; MenuItem { text: "Deep" } }
          }
        }
        """

    @api-QtQuick.Controls.Menu
    Scenario Outline: An empty menu starts with <property> <value>
      Then the property "<property>" of "empty" is <value>

      Examples:
        | property       | value |
        | count          | 0     |
        | currentIndex   | -1    |
        | title          | ""    |
        | cascade        | true  |
        | overlap        | 1     |
        | focus          | true  |
        | modal          | false |
        | visible        | false |
        | padding        | 0     |
        | margins        | 0     |
        | closePolicy    | 17    |
        | implicitWidth  | 200   |
        | implicitHeight | 40    |

    @api-QtQuick.Controls.Menu.count @api-QtQuick.Controls.Menu.itemAt
    Scenario: What is declared in a menu becomes its items, in order
      When the method "look" of "root" is called
      Then the property "count" of "m" is 5
      And the property "seen" of "root" is "One,Two,-,Act,Sub | truetrue nullnullnull | truetruetruetrue"

    Scenario Outline: In an open menu the item "<item>" has <property> <value>
      When the method "open" of "m" is called
      Then the property "<property>" of "<item>" is <value>

      Examples:
        | item | property       | value |
        | m    | width          | 200   |
        | m    | height         | 173   |
        | one  | x              | 0     |
        | one  | y              | 0     |
        | one  | width          | 200   |
        | one  | height         | 40    |
        | two  | y              | 40    |
        | line | y              | 80    |
        | line | width          | 200   |
        | line | height         | 13    |
        | line | implicitWidth  | 192   |
        | line | implicitHeight | 13    |

    @api-QtQuick.Controls.MenuItem
    Scenario Outline: A menu item starts with <property> <value>
      Then the property "<property>" of "one" is <value>

      Examples:
        | property       | value |
        | highlighted    | false |
        | checkable      | false |
        | checked        | false |
        | subMenu        | null  |
        | padding        | 6     |
        | spacing        | 6     |
        | focusPolicy    | 11    |
        | implicitWidth  | 200   |
        | implicitHeight | 40    |

    @api-QtQuick.Controls.MenuSeparator
    Scenario Outline: A separator starts with <property> <value>
      Then the property "<property>" of "line" is <value>

      Examples:
        | property            | value     |
        | padding             | 2         |
        | verticalPadding     | 6         |
        | focusPolicy         | 0         |
        | contentItem.color   | "#bdbdbd" |

    Scenario Outline: The background of a menu item is <look> with <property> <value>
      When the method "open" of "m" is called
      And the property "currentIndex" of "m" is set to <index>
      Then the property "background.<property>" of "one" is <value>

      Examples:
        | look        | index | property | value       |
        | plain       | -1    | color    | "#00000000" |
        | highlighted | 0     | color    | "#f6f6f6"   |
        | plain       | -1    | x        | 1           |
        | plain       | -1    | y        | 1           |
        | plain       | -1    | width    | 198         |
        | plain       | -1    | height   | 38          |

  Rule: Items come and go

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          id: root; objectName: "root"
          width: 400; height: 400
          property string seen: ""
          property bool free: false
          function look(got) {
            var s = []
            for (var i = 0; i < m.count; i++) s.push(m.itemAt(i).text)
            seen = s.join(",") + (got ? " got " + got.text : "")
          }
          function add() { m.addItem(loose); look() }
          function insert(i) { m.insertItem(i, loose); look() }
          function move(from, to) { m.moveItem(from, to); look() }
          function take(i) { var t = m.takeItem(i); free = !!t && t.menu === null; look(t) }
          function remove() { m.removeItem(a); look() }
          function nothing() { m.addItem(null); look() }
          MenuItem { id: loose; objectName: "loose"; text: "Loose" }
          Menu {
            id: m; objectName: "m"
            MenuItem { id: a; text: "A" }
            MenuItem { objectName: "b"; text: "B" }
          }
        }
        """

    @api-QtQuick.Controls.Menu.addItem
    Scenario: An item is added at the end
      When the method "add" of "root" is called
      Then the property "seen" of "root" is "A,B,Loose"
      And the property "count" of "m" is 3

    @api-QtQuick.Controls.Menu.insertItem
    Scenario Outline: An item inserted at <index> makes the menu "<seen>"
      When the method "insert" of "root" is called with <index>
      Then the property "seen" of "root" is "<seen>"

      Examples:
        | index | seen      |
        | 0     | Loose,A,B |
        | 1     | A,Loose,B |
        | 2     | A,B,Loose |

    @api-QtQuick.Controls.Menu.moveItem
    Scenario: An item is moved
      When the method "move" of "root" is called with 0, 1
      Then the property "seen" of "root" is "B,A"

    @api-QtQuick.Controls.Menu.removeItem
    Scenario: An item is removed
      When the method "remove" of "root" is called
      Then the property "seen" of "root" is "B"
      And the property "count" of "m" is 1

    @api-QtQuick.Controls.Menu.takeItem
    Scenario: An item is taken out and handed back
      When the method "take" of "root" is called with 0
      Then the property "seen" of "root" is "B got A"

    Scenario: Taking an item that is not there gives nothing
      When the method "take" of "root" is called with 5
      Then the property "seen" of "root" is "A,B"

    Scenario: Adding nothing adds nothing
      When the method "nothing" of "root" is called
      Then the property "seen" of "root" is "A,B"

    @api-QtQuick.Controls.MenuItem.menu
    Scenario: An item knows the menu it is in
      Then the property "menu" of "loose" is null
      And the property "menu" of "b" is not null
      When the method "add" of "root" is called
      Then the property "menu" of "loose" is not null
      When the method "take" of "root" is called with 2
      Then the property "free" of "root" is true

  Rule: Actions and menus come and go

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          id: root
          objectName: "root"
          width: 400; height: 400
          property string seen: ""
          function look(got) {
            var s = []
            for (var i = 0; i < m.count; i++) s.push(m.itemAt(i).text)
            seen = s.join(",") + (got ? " got " + (got.text || got.title) : "")
          }
          function act(i) { m.insertAction(i, extra); look() }
          function menu(i) { m.insertMenu(i, other); look() }
          function takeAct(i) { look(m.takeAction(i)) }
          function takeMenu(i) { look(m.takeMenu(i)) }
          function drop() { m.removeAction(first); m.removeMenu(sub); look() }
          function more() { m.addAction(extra); m.addMenu(other); look() }
          Action { id: extra; text: "Extra" }
          Menu { id: other; title: "Other" }
          Menu { id: m; objectName: "m"
            Action { id: first; text: "First" }
            Menu { id: sub; title: "Sub" }
          }
        }
        """

    @api-QtQuick.Controls.Menu.addAction @api-QtQuick.Controls.Menu.addMenu
    Scenario: An action and a menu are added at the end
      When the method "more" of "root" is called
      Then the property "seen" of "root" is "First,Sub,Extra,Other"

    @api-QtQuick.Controls.Menu.insertAction
    Scenario: An action is inserted
      When the method "act" of "root" is called with 0
      Then the property "seen" of "root" is "Extra,First,Sub"

    @api-QtQuick.Controls.Menu.insertMenu
    Scenario: A menu is inserted
      When the method "menu" of "root" is called with 1
      Then the property "seen" of "root" is "First,Other,Sub"

    @api-QtQuick.Controls.Menu.takeAction
    Scenario Outline: Taking the action at <index> leaves "<seen>"
      When the method "takeAct" of "root" is called with <index>
      Then the property "seen" of "root" is "<seen>"

      Examples:
        | index | seen          |
        | 0     | Sub got First |
        | 1     | First,Sub     |

    @api-QtQuick.Controls.Menu.takeMenu
    Scenario Outline: Taking the menu at <index> leaves "<seen>"
      When the method "takeMenu" of "root" is called with <index>
      Then the property "seen" of "root" is "<seen>"

      Examples:
        | index | seen          |
        | 1     | First got Sub |
        | 0     | First,Sub     |

    @api-QtQuick.Controls.Menu.removeAction @api-QtQuick.Controls.Menu.removeMenu
    Scenario: An action and a menu are removed
      When the method "drop" of "root" is called
      Then the property "seen" of "root" is ""
      And the property "count" of "m" is 0

  Rule: A menu opens where it is asked to

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          id: root
          objectName: "root"
          width: 400; height: 400
          Item {
            id: holder
            x: 50; y: 60; width: 100; height: 50
            Menu {
              id: m; objectName: "m"
              MenuItem { text: "A" }
              MenuItem { id: b; text: "B" }
              MenuItem { text: "C" }
            }
          }
          function at(x, y) { m.popup(x, y) }
          function over(x, y) { m.popup(x, y, b) }
          function inRoot(x, y) { m.popup(root, x, y) }
          function point(x, y) { m.popup(Qt.point(x, y)) }
        }
        """

    @api-QtQuick.Controls.Menu.popup
    Scenario: A menu pops up at a position in its parent
      When the method "at" of "root" is called with 30, 40
      Then the property "visible" of "m" is true
      And the property "x" of "m" is 30
      And the property "y" of "m" is 40
      And the property "currentIndex" of "m" is -1

    Scenario: A menu pops up at a point
      When the method "point" of "root" is called with 5, 6
      Then the property "x" of "m" is 5
      And the property "y" of "m" is 6

    Scenario: A menu pops up with one of its items at the position, and that item current
      When the method "over" of "root" is called with 30, 60
      Then the property "x" of "m" is 30
      And the property "y" of "m" is 20
      And the property "currentIndex" of "m" is 1

    Scenario: A menu pops up in another item
      When the method "inRoot" of "root" is called with 100, 120
      Then the property "x" of "m" is 100
      And the property "y" of "m" is 120
      And the property "parent.objectName" of "m" is "root"

    Scenario: A menu that would leave the window is moved into it
      When the method "at" of "root" is called with 340, 330
      Then the property "x" of "m" is 150
      And the property "y" of "m" is 220
      And the property "width" of "m" is 200
      And the property "height" of "m" is 120

  # Whether sub-menus cascade by default depends on the platform: they do
  # where there is a mouse cursor. The oracle's input platform has none, so
  # the menus the user works here say which they are.
  Rule: Choosing an item

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          id: root
          objectName: "root"
          width: 400; height: 400
          property string log: ""
          Action { id: act; text: "Act"; checkable: true
                   onTriggered: root.log += "act:" + checked + " " }
          Menu {
            id: m; objectName: "m"; cascade: true
            onClosed: root.log += "closed "
            MenuItem { text: "One"; onTriggered: root.log += "one " }
            MenuItem { objectName: "two"; text: "Two"; checkable: true
                       onTriggered: root.log += "two:" + checked + " " }
            MenuItem { text: "Off"; enabled: false; onTriggered: root.log += "off " }
            MenuItem { objectName: "bound"; action: act }
            Menu { objectName: "sub"; title: "Sub"
                   MenuItem { text: "Deep"; onTriggered: root.log += "deep " } }
          }
        }
        """

    @api-QtQuick.Controls.MenuItem.triggered
    Scenario: A click on an item triggers it and closes the menu
      When the method "open" of "m" is called
      And the user touches 100, 20
      Then the property "log" of "root" is "one closed "
      And the property "visible" of "m" is false
      And the property "currentIndex" of "m" is -1

    @api-QtQuick.Controls.MenuItem.checkable
    Scenario: A click on a checkable item checks it
      When the method "open" of "m" is called
      And the user touches 100, 60
      Then the property "log" of "root" is "two:true closed "
      And the property "checked" of "two" is true

    Scenario: A click on a disabled item does nothing
      When the method "open" of "m" is called
      And the user touches 100, 100
      Then the property "log" of "root" is ""
      And the property "visible" of "m" is true

    @api-QtQuick.Controls.MenuItem.action
    Scenario: An item shows its action and triggers it
      Then the property "text" of "bound" is "Act"
      And the property "checkable" of "bound" is true
      When the method "open" of "m" is called
      And the user touches 100, 140
      Then the property "log" of "root" is "act:true closed "
      And the property "checked" of "bound" is true

    @api-QtQuick.Controls.Menu.addMenu
    Scenario: A click on the item of a sub-menu opens it beside the menu
      When the method "open" of "m" is called
      And the user touches 100, 180
      Then the property "visible" of "sub" is true
      And the property "visible" of "m" is true
      And the property "x" of "sub" is 199
      And the property "y" of "sub" is 0
      And the property "currentIndex" of "m" is 4
      And the property "currentIndex" of "sub" is 0
      And the property "activeFocus" of "sub" is true
      And the property "activeFocus" of "m" is false
      And the property "log" of "root" is ""

    Scenario: Choosing an item of a sub-menu closes every menu
      When the method "open" of "m" is called
      And the user touches 100, 180
      And the user touches 300, 180
      Then the property "visible" of "sub" is false
      And the property "visible" of "m" is false
      And the property "log" of "root" is "deep closed "

    @api-QtQuick.Controls.Menu.dismiss
    Scenario: Dismissing a menu closes it and the menus above it
      When the method "open" of "m" is called
      And the user touches 100, 180
      And the method "dismiss" of "sub" is called
      Then the property "visible" of "sub" is false
      And the property "visible" of "m" is false

    Scenario: A press beside the menus closes them all
      When the method "open" of "m" is called
      And the user touches 100, 180
      And the user touches 300, 350
      Then the property "visible" of "sub" is false
      And the property "visible" of "m" is false

  Rule: A menu and the keyboard

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          id: root
          objectName: "root"
          width: 400; height: 400
          property string log: ""
          Menu {
            id: m; objectName: "m"; cascade: true
            onClosed: root.log += "closed "
            MenuItem { objectName: "one"; text: "One"; onTriggered: root.log += "one " }
            MenuSeparator {}
            MenuItem { text: "Off"; enabled: false }
            MenuItem { objectName: "two"; text: "Two"; checkable: true
                       onTriggered: root.log += "two:" + checked + " " }
            Menu { objectName: "sub"; title: "Sub"
                   MenuItem { text: "Deep"; onTriggered: root.log += "deep " }
                   MenuItem { text: "Deeper" } }
          }
        }
        """

    @api-QtQuick.Controls.Menu.currentIndex
    Scenario: An open menu has the keyboard and no current item
      When the method "open" of "m" is called
      Then the property "activeFocus" of "m" is true
      And the property "currentIndex" of "m" is -1
      And the property "highlighted" of "one" is false

    Scenario Outline: Pressing <first> then <second> makes the item <index> current
      When the method "open" of "m" is called
      And the user presses "<first>"
      And the user presses "<second>"
      Then the property "currentIndex" of "m" is <index>

      Examples:
        | first | second    | index |
        | Down  | Tab       | 3     |
        | Down  | Down      | 3     |
        | Down  | Up        | 0     |
        | Up    | Tab       | 0     |
        | Up    | Up        | -1    |
        | Up    | Down      | 0     |
        | Tab   | Tab       | 3     |
        | Tab   | Shift+Tab | 4     |

    Scenario: Down stops at the last item
      When the method "open" of "m" is called
      And the user presses "Down"
      And the user presses "Down"
      And the user presses "Down"
      And the user presses "Down"
      Then the property "currentIndex" of "m" is 4

    Scenario: Tab goes round to the first item
      When the method "open" of "m" is called
      And the user presses "Tab"
      And the user presses "Tab"
      And the user presses "Tab"
      And the user presses "Tab"
      Then the property "currentIndex" of "m" is 0

    Scenario: The current item is highlighted and has the keyboard
      When the method "open" of "m" is called
      And the user presses "Down"
      Then the property "highlighted" of "one" is true
      And the property "activeFocus" of "one" is true

    Scenario Outline: <key> triggers the current item
      When the method "open" of "m" is called
      And the user presses "Down"
      And the user presses "<key>"
      Then the property "log" of "root" is "one closed "
      And the property "visible" of "m" is false

      Examples:
        | key    |
        | Return |
        | Enter  |
        | Space  |

    Scenario: Return checks a checkable item
      When the method "open" of "m" is called
      And the user presses "Down"
      And the user presses "Down"
      And the user presses "Return"
      Then the property "log" of "root" is "two:true closed "

    Scenario: Escape closes the menu
      When the method "open" of "m" is called
      And the user presses "Escape"
      Then the property "visible" of "m" is false
      And the property "log" of "root" is "closed "

    Scenario: Right opens the sub-menu of the current item with its first item current
      When the method "open" of "m" is called
      And the user presses "Down"
      And the user presses "Down"
      And the user presses "Down"
      And the user presses "Right"
      Then the property "visible" of "sub" is true
      And the property "currentIndex" of "sub" is 0
      And the property "activeFocus" of "sub" is true

    Scenario: Left closes a sub-menu and leaves the menu open
      When the method "open" of "m" is called
      And the user presses "Down"
      And the user presses "Down"
      And the user presses "Down"
      And the user presses "Right"
      And the user presses "Left"
      Then the property "visible" of "sub" is false
      And the property "visible" of "m" is true
      And the property "currentIndex" of "m" is 4

    Scenario: Return on the item of a sub-menu opens it
      When the method "open" of "m" is called
      And the user presses "Down"
      And the user presses "Down"
      And the user presses "Down"
      And the user presses "Return"
      Then the property "visible" of "sub" is true
      And the property "visible" of "m" is true

    Scenario: Escape in a sub-menu closes that menu only
      When the method "open" of "m" is called
      And the user presses "Down"
      And the user presses "Down"
      And the user presses "Down"
      And the user presses "Right"
      And the user presses "Escape"
      Then the property "visible" of "sub" is false
      And the property "visible" of "m" is true

    Scenario: Return in a sub-menu triggers its item and closes every menu
      When the method "open" of "m" is called
      And the user presses "Down"
      And the user presses "Down"
      And the user presses "Down"
      And the user presses "Right"
      And the user presses "Return"
      Then the property "log" of "root" is "deep closed "
      And the property "visible" of "m" is false

    Scenario: Right on an item without a sub-menu and Left in a top menu do nothing
      When the method "open" of "m" is called
      And the user presses "Down"
      And the user presses "Right"
      And the user presses "Left"
      Then the property "visible" of "m" is true
      And the property "visible" of "sub" is false
      And the property "currentIndex" of "m" is 0

    Scenario: Down moves inside the sub-menu that has the keyboard
      When the method "open" of "m" is called
      And the user presses "Down"
      And the user presses "Down"
      And the user presses "Down"
      And the user presses "Right"
      And the user presses "Down"
      Then the property "currentIndex" of "sub" is 1
      And the property "currentIndex" of "m" is 4

  Rule: A menu that does not cascade

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          id: root
          objectName: "root"
          width: 400; height: 400
          property string log: ""
          Menu {
            id: m; objectName: "m"; cascade: false
            onClosed: root.log += "closed "
            onOpened: root.log += "opened "
            MenuItem { text: "One" }
            Menu { objectName: "sub"; title: "Sub"
                   onClosed: root.log += "subclosed "
                   MenuItem { text: "Deep"; onTriggered: root.log += "deep " } }
          }
        }
        """

    @api-QtQuick.Controls.Menu.cascade
    Scenario: A sub-menu cascades as its menu does
      Then the property "cascade" of "sub" is false

    Scenario: The sub-menu takes the place of its menu
      When the method "open" of "m" is called
      And the user touches 100, 60
      Then the property "visible" of "sub" is true
      And the property "visible" of "m" is false
      And the property "log" of "root" is "opened closed "

    Scenario: Left goes back to the menu
      When the method "open" of "m" is called
      And the user presses "Down"
      And the user presses "Down"
      And the user presses "Right"
      And the user presses "Left"
      Then the property "visible" of "sub" is false
      And the property "visible" of "m" is true
      And the property "currentIndex" of "m" is 1
      And the property "log" of "root" is "opened closed subclosed opened "

    Scenario: Escape in the sub-menu leaves every menu closed
      When the method "open" of "m" is called
      And the user presses "Down"
      And the user presses "Down"
      And the user presses "Right"
      And the user presses "Escape"
      Then the property "visible" of "sub" is false
      And the property "visible" of "m" is false

    Scenario: Choosing an item of the sub-menu closes it
      When the method "open" of "m" is called
      And the user presses "Down"
      And the user presses "Down"
      And the user presses "Return"
      And the user presses "Return"
      Then the property "visible" of "sub" is false
      And the property "visible" of "m" is false
      And the property "log" of "root" is "opened closed deep subclosed "

    Scenario: The current item is kept when the menu closes
      When the method "open" of "m" is called
      And the user presses "Down"
      And the method "close" of "m" is called
      Then the property "currentIndex" of "m" is 0

  Rule: The parts of an item

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          id: root
          objectName: "root"
          width: 400; height: 400
          property string seen: ""
          function look() {
            var s = []
            for (var i = 0; i < m.count; i++) {
              var it = m.itemAt(i)
              s.push((it.arrow.parent === it) + " " + it.arrow.visible + " " + it.indicator.visible
                     + " " + (it.arrow.x > it.width / 2))
            }
            seen = s.join(",")
          }
          Menu {
            id: m; objectName: "m"
            MenuItem { text: "One" }
            MenuItem { text: "Two"; checkable: true }
            MenuItem { text: "Three"; checkable: true; checked: true }
            Menu { title: "Sub"; MenuItem { text: "Deep" } }
          }
        }
        """

    @api-QtQuick.Controls.MenuItem
    Scenario: Only an item that opens a menu shows an arrow, and only a checked one an indicator
      When the method "open" of "m" is called
      And the method "look" of "root" is called
      Then the property "seen" of "root" is "true false false true,true false false true,true false true true,true true false true"

  Rule: A menu with more items than fit in the window scrolls

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          id: root; objectName: "root"
          width: 400; height: 400
          property string chosen: ""
          property bool dragged: m.contentItem.contentY > 0
          Menu {
            id: m; objectName: "m"
            Repeater {
              model: 20
              MenuItem { text: "Item " + index; onTriggered: root.chosen = text }
            }
          }
        }
        """

    Scenario: The menu is no taller than the window
      When the method "open" of "m" is called
      Then the property "height" of "m" is 400
      And the property "y" of "m" is 0
      And the property "count" of "m" is 20

    Scenario: The items are in a view that is as tall as the menu
      When the method "open" of "m" is called
      Then the property "contentItem.height" of "m" is 400
      And the property "contentItem.contentHeight" of "m" is 800
      And the property "contentItem.contentY" of "m" is 0
      And the property "contentItem.clip" of "m" is true
      And the property "implicitHeight" of "m" is 800

    Scenario: An item that becomes current is brought into view
      When the method "open" of "m" is called
      And the property "currentIndex" of "m" is set to 19
      Then the property "contentItem.contentY" of "m" is 400

    Scenario: A key that goes past the last item in view moves the items up
      When the method "open" of "m" is called
      And the property "currentIndex" of "m" is set to 9
      And the user presses "Down"
      And 1 second passes
      Then the property "currentIndex" of "m" is 10
      And the property "contentItem.contentY" of "m" is 40

    Scenario: A drag over the items moves them and chooses none
      When the method "open" of "m" is called
      And the user drags from 100, 300 to 100, 170
      Then the property "dragged" of "root" is true
      And the property "visible" of "m" is true
      And the property "chosen" of "root" is ""

    Scenario: A touch chooses the item that is seen there
      When the method "open" of "m" is called
      And the property "currentIndex" of "m" is set to 19
      And the user touches 100, 20
      Then the property "chosen" of "root" is "Item 10"
      And the property "visible" of "m" is false
