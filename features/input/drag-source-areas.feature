# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-drag.html#source-attached-prop
#   https://doc.qt.io/qt-6/qml-qtquick-droparea.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A drag is not offered to the areas of its source
  The DropAreas inside the drag's source, and the source itself, are passed
  over: an item that holds a DropArea of its own is dragged onto the areas
  under it, not onto itself. The source is the dragged item unless
  Drag.source says otherwise.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        width: 300; height: 300
        property string log: ""
        property bool on: false
        property bool other: false
        Rectangle {
          id: under
          y: 100; width: 100; height: 100
          DropArea { anchors.fill: parent; onEntered: root.log += "enter under|" }
        }
        Rectangle {
          id: dragged
          objectName: "dragged"
          width: 100; height: 100; z: 1
          Drag.active: root.on
          Drag.source: root.other ? under : dragged
          Drag.hotSpot.x: 50; Drag.hotSpot.y: 50
          DropArea { anchors.fill: parent; onEntered: root.log += "enter own|" }
          Item { DropArea { width: 100; height: 100; onEntered: root.log += "enter inner|" } }
        }
      }
      """

  Scenario: The dragged item's own areas are passed over
    When the property "on" of "root" is set to true
    And 100 milliseconds pass
    And the property "y" of "dragged" is set to 100
    And 100 milliseconds pass
    Then the property "log" of "root" is "enter under|"

  Scenario: The areas of another source are passed over instead
    When the property "other" of "root" is set to true
    And the property "on" of "root" is set to true
    And 100 milliseconds pass
    And the property "y" of "dragged" is set to 100
    And 100 milliseconds pass
    Then the property "log" of "root" is "enter inner|"
