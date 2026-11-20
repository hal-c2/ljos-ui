# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-menu.html
#   https://doc.qt.io/qt-6/qml-qtqml-models-instantiator.html
# Verified on Qt 6.12.0 (qml6, offscreen).

@qt-parity
Feature: Items an Instantiator adds to a menu
  The items declared in a menu are its own from the start, so the index
  an `onObjectAdded` handler gives counts them, even when it runs before
  the menu is complete.

  Rule: Items inserted after a declared item

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          id: root; objectName: "root"
          property string seen: ""
          function look() {
            var s = []; for (var i = 0; i < m.count; i++) s.push(m.itemAt(i).text); seen = s.join(",")
          }
          function grow() { lm.append({ name: "P3" }); look() }
          function shrink() { lm.remove(0); look() }
          ListModel { id: lm; ListElement { name: "P1" } ListElement { name: "P2" } }
          Menu {
            id: m
            MenuItem { text: "All" }
            Instantiator {
              model: lm
              delegate: MenuItem { text: name }
              onObjectAdded: (index, object) => m.insertItem(index + 1, object)
              onObjectRemoved: (index, object) => m.removeItem(object)
            }
            MenuItem { text: "End" }
          }
          Component.onCompleted: look()
        }
        """

    @api-QtQuick.Controls.Menu.insertItem
    Scenario: Items inserted while the menu is made count the declared items
      Then the property "seen" of "root" is "All,P1,P2,End"

    @api-QtQuick.Controls.Menu.insertItem
    Scenario: A row added to the model later is inserted at its place
      When the method "grow" of "root" is called
      Then the property "seen" of "root" is "All,P1,P2,P3,End"

    @api-QtQuick.Controls.Menu.removeItem
    Scenario: A row removed from the model takes its item out
      When the method "shrink" of "root" is called
      Then the property "seen" of "root" is "All,P2,End"

  Rule: Items added after declared actions and menus

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          id: root; objectName: "root"
          property string seen: ""
          Menu {
            id: m
            Action { text: "Act" }
            Instantiator {
              model: ["P1", "P2"]
              delegate: MenuItem { required property var modelData; text: modelData }
              onObjectAdded: (index, object) => m.addItem(object)
            }
            Menu { title: "Sub" }
          }
          Component.onCompleted: {
            var s = []
            for (var i = 0; i < m.count; i++) s.push(m.itemAt(i).text)
            seen = s.join(",")
          }
        }
        """

    @api-QtQuick.Controls.Menu.addItem
    Scenario: Items added while the menu is made come after every declared item
      Then the property "seen" of "root" is "Act,Sub,P1,P2"
