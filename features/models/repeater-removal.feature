# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-repeater.html
#   https://doc.qt.io/qt-6/qml-qtqml-component.html#destruction-signal
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A removed Repeater delegate goes without running against a missing parent
  The delegate of a removed row hears itemRemoved and Component.onDestruction while it is still in its parent, then leaves the tree. From then on its bindings keep the values they had: they and its anchors do not run again against a parent it no longer has.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 200; height: 100
        property bool flag: false; property string log: ""
        property int kids: children.length; property var gone; property string seen: ""
        function removeFirst() {
          lm.remove(0); width = 300
          seen = gone.parent + " " + gone.w + " " + gone.row
        }
        ListModel {
          id: lm; objectName: "lm"
          ListElement { name: "a" }
          ListElement { name: "b" }
        }
        Repeater {
          model: lm
          onItemRemoved: (i, item) => { root.log += "-" + (item.parent === root); root.gone = item }
          Item {
            objectName: "d-" + model.name; width: parent.width; height: 10
            property int row: index; property real w: parent ? parent.width : -5
            anchors.top: root.flag ? undefined : parent.top
            Component.onDestruction: root.log += "~" + (parent === root)
          }
        }
      }
      """
    When 100 milliseconds pass

  Scenario: The delegate is told while it is still in its parent
    When the method "remove" of "lm" is called with 0
    Then the property "log" of "root" is "-true~true"
    And the property "kids" of "root" is 2

  Scenario: Removing a row logs nothing
    When the method "remove" of "lm" is called with 0
    And 100 milliseconds pass
    Then the object "d-a" does not exist
    And the property "width" of "d-b" is 200
    And the log has no warnings

  Scenario: A removed delegate keeps the values its bindings had
    When the method "removeFirst" of "root" is called
    Then the property "seen" of "root" is "null 200 -1"
