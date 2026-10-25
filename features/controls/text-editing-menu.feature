# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-contextmenu.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-textarea.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-textfield.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: The edit menu of a text field and a text area
  A right-click on a TextArea or a TextField opens a menu of its own:
  Undo, Redo, Cut, Copy, Paste, Delete and Select All, each enabled as
  the editor allows. Cut and Delete need a selection in an editor that
  can be changed, Copy a selection, Paste something to paste, and Select
  All is always there. The menu opens at the text cursor, not where the
  click was, and is the editor's `ContextMenu.menu` once it has opened;
  a menu an author gives instead is opened in its place. Unlike other
  items, an editor opens its menu even when `requested` is handled.

  # The menus are read from the overlay when a step asks: qml6 breaks a
  # context menu that a binding reads before it is first opened. qml6 also
  # stops answering when an edit menu is pressed in the test harness, so
  # `choose` triggers the item as a click on it would.
  Background:
    Given the file "MenuReader.qml":
      """qml
      import QtQuick
      import QtQuick.Controls
      Item {
        property Item editor
        property var ov: Overlay.overlay
        property string items: ""
        property bool shown
        property bool atCursor
        function entries(n, out) {
          for (var i = 0; i < n.children.length; i++) {
            const c = n.children[i]
            if (!c.visible) continue
            if ("triggered" in c && "highlighted" in c) out.push(c); else entries(c, out)
          }
          return out
        }
        function list() { items = entries(ov, []).map(c => c.text + (c.enabled ? "" : " (off)")).join(", ") }
        function choose(t) { const c = entries(ov, []).find(c => c.text === t); if (c.action) c.action.trigger(); else c.click() }
        function look() {
          const m = editor.ContextMenu.menu, r = editor.cursorRectangle
          shown = m !== null && m.opened
          atCursor = m !== null && m.x === Math.round(r.x + r.width / 2) && m.y === Math.round(r.y + r.height / 2)
        }
      }
      """
    And the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        id: root; objectName: "root"
        width: 400; height: 400
        property string log: ""
        function type(t) { area.forceActiveFocus(); area.insert(area.length, t) }
        MenuReader { objectName: "reader"; editor: area }
        TextArea { id: area; objectName: "area"; width: 200; height: 100; text: "select me" }
        TextArea { id: fixed; objectName: "fixed"; y: 110; width: 200; height: 60; text: "fixed"; readOnly: true }
        TextField {
          objectName: "field"; y: 180; width: 200; height: 40
          ContextMenu.onRequested: (p) => root.log += "told " + p.x + "," + p.y + ";"
        }
        TextArea {
          objectName: "own"; y: 230; width: 200; height: 60; text: "own"
          ContextMenu.menu: Menu { MenuItem { text: "Mine" } }
          ContextMenu.onRequested: root.log += "mine;"
        }
        Component.onCompleted: { fixed.selectAll(); fixed.copy(); fixed.deselect() }
      }
      """

  Rule: The items are enabled as the editor allows

    Scenario: With nothing selected only Paste and Select All are enabled
      When the user clicks the right button at 150, 60
      And the method "list" of "reader" is called
      And the method "look" of "reader" is called
      Then the property "items" of "reader" is "Undo (off), Redo (off), Cut (off), Copy (off), Paste, Delete (off), Select All"
      And the property "shown" of "reader" is true

    Scenario: A selection can be cut, copied and deleted
      When the method "selectAll" of "area" is called
      And the user clicks the right button at 150, 60
      And the method "list" of "reader" is called
      Then the property "items" of "reader" is "Undo (off), Redo (off), Cut, Copy, Paste, Delete, Select All"

    Scenario: A read-only editor's selection can only be copied
      When the method "selectAll" of "fixed" is called
      And the user clicks the right button at 150, 140
      And the method "list" of "reader" is called
      Then the property "items" of "reader" is "Undo (off), Redo (off), Cut (off), Copy, Paste (off), Delete (off), Select All"

    Scenario: A change can be undone, and an undone change redone
      When the method "type" of "root" is called with "!"
      And the user clicks the right button at 150, 60
      And the method "list" of "reader" is called
      Then the property "items" of "reader" is "Undo, Redo (off), Cut (off), Copy (off), Paste, Delete (off), Select All"
      When the user presses "Escape"
      And the method "undo" of "area" is called
      And the user clicks the right button at 150, 60
      And the method "list" of "reader" is called
      Then the property "items" of "reader" is "Undo (off), Redo, Cut (off), Copy (off), Paste, Delete (off), Select All"

  Rule: Each item does what the editor's method of that name does

    Scenario: Copy puts the selection on the clipboard
      When the method "selectAll" of "area" is called
      And the user clicks the right button at 150, 60
      And the method "choose" of "reader" is called with "Copy"
      And the method "look" of "reader" is called
      Then the clipboard holds "select me"
      And the property "text" of "area" is "select me"
      And the property "shown" of "reader" is false

    Scenario: Cut takes the selection to the clipboard
      When the method "selectAll" of "area" is called
      And the user clicks the right button at 150, 60
      And the method "choose" of "reader" is called with "Cut"
      Then the clipboard holds "select me"
      And the property "text" of "area" is ""

    Scenario: Delete takes the selection away and leaves the clipboard
      When the method "select" of "area" is called with 0, 7
      And the user clicks the right button at 150, 60
      And the method "choose" of "reader" is called with "Delete"
      Then the property "text" of "area" is "me"
      And the clipboard holds "fixed"

    Scenario: Paste puts the clipboard at the cursor
      When the property "cursorPosition" of "area" is set to 6
      And the user clicks the right button at 150, 60
      And the method "choose" of "reader" is called with "Paste"
      Then the property "text" of "area" is "selectfixed me"

    Scenario: Select All selects the whole text
      When the user clicks the right button at 150, 60
      And the method "choose" of "reader" is called with "Select All"
      Then the property "selectedText" of "area" is "select me"

    Scenario: Undo takes back the last change
      When the method "type" of "root" is called with "!"
      And the user clicks the right button at 150, 60
      And the method "choose" of "reader" is called with "Undo"
      Then the property "text" of "area" is "select me"

  Rule: The menu opens at the text cursor

    Scenario: The menu opens at the cursor, not where the click was
      When the property "cursorPosition" of "area" is set to 0
      And the user clicks the right button at 150, 60
      And the method "look" of "reader" is called
      Then the property "atCursor" of "reader" is true
      And the property "activeFocus" of "area" is false

    Scenario: A request is told the cursor's place, and the edit menu opens all the same
      When the user clicks the right button at 150, 200
      And the method "list" of "reader" is called
      Then the property "log" of "root" is "told 11,20;"
      And the property "items" of "reader" is "Undo (off), Redo (off), Cut (off), Copy (off), Paste, Delete (off), Select All"

    Scenario: A menu the author gives opens instead, handled or not
      When the user clicks the right button at 150, 260
      And the method "list" of "reader" is called
      Then the property "items" of "reader" is "Mine"
      And the property "log" of "root" is "mine;"
