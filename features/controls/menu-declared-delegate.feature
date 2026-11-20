# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-menu.html#delegate-prop
# Verified on Qt 6.12.0 (qml6, offscreen).

@qt-parity
Feature: The delegate that makes the items of a menu's declared actions
  A menu makes an item for each action and sub-menu declared in it while
  the menu is made. A delegate written in place makes those items. A
  delegate given by a binding comes too late for them, so the style's own
  delegate makes them; it makes the items of what is added later.

  Rule: A delegate given by a binding

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          id: root; objectName: "root"
          property string seen: ""
          property string last: ""
          function names() { var s = []; for (var i = 0; i < m.count; i++) s.push(m.itemAt(i).objectName + ":" + m.itemAt(i).text); return s.join(",") }
          function addLater() { m.addAction(later); last = m.itemAt(m.count - 1).objectName }
          Component { id: custom; MenuItem { objectName: "c" } }
          Action { id: later; text: "Later" }
          Menu {
            id: m
            delegate: custom
            Action { text: "Act" }
            Menu { title: "Sub" }
            Instantiator {
              model: 1; delegate: MenuItem { text: "Gen" }
              onObjectAdded: (index, object) => m.addItem(object)
            }
          }
          Component.onCompleted: Qt.callLater(function() { seen = names() })
        }
        """

    @api-QtQuick.Controls.Menu.delegate
    Scenario: The declared actions and menus get their items from the style's delegate
      Then the property "seen" of "root" is ":Act,:Sub,:Gen"

    @api-QtQuick.Controls.Menu.delegate @api-QtQuick.Controls.Menu.addAction
    Scenario: An action added once the menu is made gets its item from the bound delegate
      When the method "addLater" of "root" is called
      Then the property "last" of "root" is "c"

  Rule: A delegate written in place

    @api-QtQuick.Controls.Menu.delegate
    Scenario: The declared actions and menus get their items from it
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          id: root; objectName: "root"
          property string seen: ""
          Menu {
            id: m
            delegate: Component { MenuItem { objectName: "c" } }
            Action { text: "Act" }
            Menu { title: "Sub" }
            MenuItem { text: "It" }
          }
          Component.onCompleted: Qt.callLater(function() {
            var s = []; for (var i = 0; i < m.count; i++) s.push(m.itemAt(i).objectName + ":" + m.itemAt(i).text); seen = s.join(",")
          })
        }
        """
      Then the property "seen" of "root" is "c:Act,c:Sub,:It"
