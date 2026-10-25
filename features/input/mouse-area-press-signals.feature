# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-mousearea.html#pressed-signal
#   https://doc.qt.io/qt-6/qml-qtquick-mousearea.html#containsPress-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: When a MouseArea tells of its press
  pressed and released come before pressedChanged, though their handlers
  read the new value of pressed: a drag whose Drag.active is bound to
  pressed can still be dropped in onReleased. A press that is turned down
  tells it went back up. containsPress is pressed while the pointer is
  inside.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        width: 300; height: 300
        property string log: ""
        property bool refuse: false
        DropArea { anchors.fill: parent; onDropped: root.log += "dropped|" }
        Rectangle {
          width: 100; height: 100
          Drag.active: area.pressed
          Drag.hotSpot.x: 50; Drag.hotSpot.y: 50
          MouseArea {
            id: area
            objectName: "area"
            anchors.fill: parent
            onPressed: mouse => { mouse.accepted = !root.refuse; root.log += "pressed " + pressed + "|" }
            onReleased: { parent.Drag.drop(); root.log += "released " + pressed + "|" }
            onPressedChanged: root.log += "pc " + pressed + "|"
            onContainsPressChanged: root.log += "cp " + containsPress + "|"
            onClicked: root.log += "clicked|"
          }
        }
      }
      """

  Scenario: A click tells of the press after pressed and of the release after released
    When the user presses the pointer at 50, 50
    And the user releases the pointer at 50, 50
    Then the property "log" of "root" is "pressed true|pc true|cp true|dropped|released false|pc false|cp false|clicked|"

  Scenario: A press that is turned down tells it went back up
    When the property "refuse" of "root" is set to true
    And the user presses the pointer at 50, 50
    Then the property "log" of "root" is "pressed true|pc false|cp false|"
    And the property "pressed" of "area" is false

  Scenario: Leaving the area while pressed ends containsPress only
    When the user presses the pointer at 50, 50
    And the property "log" of "root" is set to ""
    And the user moves the pointer to 200, 200
    Then the property "log" of "root" is "cp false|"
    And the property "pressed" of "area" is true
    And the property "containsPress" of "area" is false
