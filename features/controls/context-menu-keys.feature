# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-contextmenu.html
#
# The documentation has the Menu key and Shift+F10 ask for a context
# menu as a right-click does; qml6 6.11.2 on Linux (offscreen) opens
# nothing for either, so the oracle cannot observe this.

@ljos-only
Feature: Asking for a context menu from the keyboard
  The Menu key, or Shift+F10, asks the item with active focus for its
  context menu, as a right-click asks the item under the pointer. The
  request is at the middle of the item, and an item that takes no
  requests passes it to the item it is in. An editor opens its edit
  menu at its text cursor. A key handler that takes the key first keeps
  it.

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
        TextArea { id: area; objectName: "area"; width: 200; height: 100; text: "select me" }
        MenuReader { objectName: "reader"; editor: area }
        Rectangle {
          objectName: "told"; y: 120; width: 100; height: 60
          ContextMenu.onRequested: (p) => root.log += "told " + p.x + "," + p.y + ";"
        }
        Rectangle {
          y: 200; width: 200; height: 80
          ContextMenu.onRequested: (p) => root.log += "outer " + p.x + "," + p.y + ";"
          Rectangle { objectName: "inner"; width: 20; height: 20 }
        }
        Rectangle {
          objectName: "keys"; y: 300; width: 50; height: 50
          ContextMenu.onRequested: root.log += "asked;"
          Keys.onPressed: (e) => { if (e.key === Qt.Key_Menu) { root.log += "keys;"; e.accepted = true } }
        }
      }
      """

  Scenario: The Menu key asks the focused item, at its middle
    When the method "forceActiveFocus" of "told" is called
    And the user presses "Menu"
    Then the property "log" of "root" is "told 50,30;"

  Scenario: Shift+F10 asks as the Menu key does
    When the method "forceActiveFocus" of "told" is called
    And the user presses "Shift+F10"
    Then the property "log" of "root" is "told 50,30;"

  Scenario: F10 alone asks for nothing
    When the method "forceActiveFocus" of "told" is called
    And the user presses "F10"
    Then the property "log" of "root" is ""

  Scenario: An item that takes no requests passes it to the item it is in
    When the method "forceActiveFocus" of "inner" is called
    And the user presses "Menu"
    Then the property "log" of "root" is "outer 100,40;"

  Scenario: A key handler that takes the key keeps it
    When the method "forceActiveFocus" of "keys" is called
    And the user presses "Menu"
    Then the property "log" of "root" is "keys;"

  Scenario: An editor opens its edit menu at its text cursor
    When the method "forceActiveFocus" of "area" is called
    And the user presses "Menu"
    And the method "look" of "reader" is called
    Then the property "shown" of "reader" is true
    And the property "atCursor" of "reader" is true
