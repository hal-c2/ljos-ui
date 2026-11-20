# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#visible-prop
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#enabled-prop
#   https://doc.qt.io/qt-6/qml-qtqml-component.html#createObject-method
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: An item made by a script inherits before the scene shows it
  An item made with createObject is read by the script that made it before
  any frame shows it. What it and its children inherit follows their
  parents from the start, and a parent shown or enabled again in that same
  script gives the children their own values back.

  Scenario: A child whose own value turns true under a new hidden parent shows with it
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property string log: ""
        function hideThenChange() {
          const made = shown.createObject(root)
          made.visible = false
          made.icon = "ask"
          log += made.children[0].visible + " "
          made.visible = true
          log += made.children[0].visible + " "
        }
        Component {
          id: shown
          Item { property string icon: ""
            Item { visible: parent.icon !== "" } }
        }
      }
      """
    When the method "hideThenChange" of "root" is called
    Then the property "log" of "root" is "false true "

  Scenario: An item that hides itself as it completes under a disabled parent gives its children back
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property string log: ""
        function note(c) { log += c.visible + " " + c.enabled + " " }
        function hiddenAtCompletion() {
          const made = hidden.createObject(panel)
          note(made.children[0])
          made.visible = true
          note(made.children[0])
          panel.enabled = true
          note(made.children[0])
        }
        Component {
          id: hidden
          Item { id: over; property string icon: ""
            Component.onCompleted: { visible = false; icon = "ask" }
            Item { visible: over.icon !== "" } }
        }
        Item { id: panel; enabled: false }
      }
      """
    When the method "hiddenAtCompletion" of "root" is called
    Then the property "log" of "root" is "false false true false true true "
