# Sources:
#   https://doc.qt.io/qt-6/qtquick-positioning-anchors.html
#   https://doc.qt.io/qt-6/qml-qtquick-column.html
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#implicitWidth-prop
# Verified on Qt 6.11.2 (qml6, offscreen).
# Found by ljos-fuzz (geometry), buckets value:step-anchors:number, value:step-fill:number,
# value:step-center-in:number, throws:TypeError/none:step-set, value:load-in-positioner:rounding
# and value:load-positioner:rounding.

@qt-parity
Feature: What anchors or a positioner place, a write does not move
  A coordinate or size that an item's anchors or its positioner set stays where they set
  it when a script writes it. A positioner's implicit size is its own, worked out from its
  children, negative sizes too, and cannot be written at all. A size bound in QML follows
  its literal implicit size first, then its binding, and both changes are told.

  @backlog
  Scenario: An anchored edge keeps the item where its anchors put it
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        height: 200
        Item {
          id: child
          objectName: "child"
          anchors.top: parent.verticalCenter
          function move() { y = 40 }
        }
      }
      """
    When the method "move" of "child" is called
    Then the property "y" of "child" is 100

  @backlog
  Scenario: A filled item keeps its parent's size
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        width: 60
        height: 30
        Item {
          objectName: "child"
          anchors.fill: parent
          function grow() { height = 10 }
        }
      }
      """
    When the method "grow" of "child" is called
    Then the property "height" of "child" is 30

  @backlog
  Scenario: A centred item keeps its centre
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        width: 100
        height: 100
        Item {
          objectName: "child"
          width: 20
          height: 20
          anchors.centerIn: parent
          function move() { y = 0 }
        }
      }
      """
    When the method "move" of "child" is called
    Then the property "y" of "child" is 40

  @backlog
  Scenario Outline: A <type>'s implicit size cannot be written
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string thrown: ""
        <type> { id: positioner }
        function write() {
          try { positioner.implicitWidth = 40 } catch (e) { thrown = e.name }
        }
      }
      """
    When the method "write" of "root" is called
    Then the property "thrown" of "root" is "TypeError"

    Examples:
      | type   |
      | Row    |
      | Column |

  @backlog
  Scenario: A child's negative size counts in its positioner's size
    Given the QML:
      """qml
      import QtQuick
      Item {
        Column {
          objectName: "column"
          Rectangle { width: 20; implicitHeight: -10 }
        }
        Row {
          objectName: "row"
          Rectangle { width: -7; height: 5 }
          Rectangle { width: 3; height: -4 }
        }
      }
      """
    Then the property "implicitHeight" of "column" is -10
    And the property "implicitWidth" of "row" is -4
    And the property "implicitHeight" of "row" is 5

  # Found by ljos-fuzz (geometry), bucket value:onChanged:empty-ljos.
  @backlog
  Scenario: A size bound to what it already follows from its implicit size is told
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string log: ""
        Item { width: Math.sqrt(0); implicitWidth: 40; onWidthChanged: log += "b" + width + ";" }
        Item { implicitWidth: 40; width: Math.sqrt(0); onWidthChanged: log += "c" + width + ";" }
        Item { width: 0; implicitWidth: 40; onWidthChanged: log += "a" + width + ";" }
      }
      """
    Then the property "log" of "root" is "b0;c0;"
