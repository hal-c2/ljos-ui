# Sources:
#   https://doc.qt.io/qt-6/qtquick-positioning-anchors.html
#   https://doc.qt.io/qt-6/qml-qtquick-column.html
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#implicitWidth-prop
# Verified on Qt 6.11.2 (qml6, offscreen).
# Found by ljos-fuzz (geometry), buckets value:step-anchors:number, value:step-fill:number,
# value:step-center-in:number, throws:TypeError/none:step-set, value:load-in-positioner:rounding,
# value:load-positioner:rounding, value:load-handlers-fill:number and
# value:load-handlers-center-in:number.

@qt-parity
Feature: What anchors or a positioner place, a write does not move
  A coordinate or size that an item's anchors or its positioner set stays where they set
  it when a script writes it, and the change is told twice, at the value the anchors keep. A
  positioner's implicit size is its own, worked out from its
  children, negative sizes too, and cannot be written at all. A size bound in QML follows
  its literal implicit size first, then its binding, and both changes are told.

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

  Scenario: A write as the root completes does not move a filled item
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        Component.onCompleted: child.x = 3
        Item {
          id: child
          objectName: "child"
          anchors.fill: parent
        }
      }
      """
    Then the property "x" of "child" is 0

  Scenario: A write as a centred item completes does not move it
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        Item {
          id: child
          objectName: "child"
          anchors.centerIn: parent
          Component.onCompleted: child.y = 3
        }
      }
      """
    Then the property "y" of "child" is 0

  Scenario: A write the anchors undo is told twice, at the value they keep
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property string log: ""
        Item {
          id: child
          objectName: "child"
          anchors.top: parent.top
          anchors.topMargin: 5
          onYChanged: root.log += y + ";"
        }
        function move() { log = ""; child.y = 7 }
        function same() { log = ""; child.y = 5 }
        function refused() { log = ""; child.y = NaN }
      }
      """
    When the method "move" of "root" is called
    Then the property "log" of "root" is "5;5;"
    When the method "same" of "root" is called
    Then the property "log" of "root" is ""
    When the method "refused" of "root" is called
    Then the property "log" of "root" is ""

  Scenario: What anchors do not place, a write moves
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        height: 200
        Item {
          id: child
          objectName: "child"
          anchors.top: parent.top
          function move() { x = 7; y = 40 }
        }
      }
      """
    When the method "move" of "child" is called
    Then the property "x" of "child" is 7
    And the property "y" of "child" is 0

  Scenario: A write the anchors undid does not come back when they go
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        Item {
          id: child
          objectName: "child"
          anchors.top: parent.top
          anchors.topMargin: 5
          function move() { y = 7; anchors.top = undefined }
          function free() { y = 9 }
        }
      }
      """
    When the method "move" of "child" is called
    Then the property "y" of "child" is 5
    When the method "free" of "child" is called
    Then the property "y" of "child" is 9

  Scenario: A size taken back from a filled item stays its parent's, and is told twice
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        width: 100
        property string log: ""
        Item {
          objectName: "child"
          anchors.fill: parent
          onWidthChanged: root.log += width + ";"
          function takeBack() { root.log = ""; width = undefined }
        }
      }
      """
    When the method "takeBack" of "child" is called
    Then the property "width" of "child" is 100
    And the property "log" of "root" is "100;100;"

  Scenario: A Behavior does not move what anchors place
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        Item {
          id: child
          objectName: "child"
          anchors.top: parent.top
          anchors.topMargin: 5
          Behavior on y { NumberAnimation { duration: 10 } }
          function move() { y = 30 }
        }
      }
      """
    When the method "move" of "child" is called
    And 50 milliseconds pass
    Then the property "y" of "child" is 5

  # An animation's steps are undone like a script's writes, each told twice.
  @backlog
  Scenario: An animation does not move what anchors place
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        Item {
          objectName: "child"
          anchors.top: parent.top
          anchors.topMargin: 5
          NumberAnimation on y { from: 20; to: 40; duration: 10 }
        }
      }
      """
    When 50 milliseconds pass
    Then the property "y" of "child" is 5

  # Qt keeps a binding a script gives while anchored, undoing what it gives; once the anchors
  # go it runs at the next change of what it reads. Ljós forgets it with the anchors.
  @backlog
  Scenario: A binding given under anchors runs once they are gone
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property int to: 3
        Item {
          id: child
          objectName: "child"
          anchors.fill: parent
          function follow() { x = Qt.binding(function () { return root.to }) }
          function free() { anchors.fill = undefined; root.to = 10 }
        }
      }
      """
    When the method "follow" of "child" is called
    Then the property "x" of "child" is 0
    When the method "free" of "child" is called
    Then the property "x" of "child" is 10

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
