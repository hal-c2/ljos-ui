# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#x-prop
# Verified on Qt 6.11.2 (qml6, offscreen).
# Found by ljos-fuzz (bindings), buckets value:bool:isNaN, value:double:default,
# value:double:ref, value:var(number):ref and value:point:step-set(Qt.point).

@qt-parity
Feature: An item's geometry is never NaN
  `x`, `y`, `width` and `height` given NaN keep the value they had: nothing changes and no
  change is told, whether a binding or a script gives it.

  Scenario: A binding that gives NaN leaves the geometry where it was
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        x: parseFloat("")
        y: parseInt("", 16)
        property int moves: 0
        onXChanged: moves++
        property bool yIsNaN: isNaN(y)
      }
      """
    Then the property "x" of "root" is 0
    And the property "moves" of "root" is 0
    And the property "yIsNaN" of "root" is false

  Scenario: A script that gives NaN leaves the size where it was
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        width: 20
        function spoil() { width = NaN; height = parseInt("") }
      }
      """
    When the method "spoil" of "root" is called
    Then the property "width" of "root" is 20
    And the property "height" of "root" is 0

  # Found by ljos-fuzz (bindings), bucket value:double:step-set(ref).
  @backlog
  Scenario Outline: <property> given NaN is NaN
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property bool nan: isNaN(<property>)
        function give() { <property> = NaN }
      }
      """
    When the method "give" of "root" is called
    Then the property "nan" of "root" is true

    Examples:
      | property |
      | rotation |
      | scale    |
      | opacity  |
      | z        |

  # Found by ljos-fuzz (bindings), bucket warning:assign(ljos-only):Cannot_assign_double_to_Text::HAlignment,
  # (geometry) value:load-handlers-free:int-range: the handler that would fail never runs.
  Scenario: The handler of a size given NaN does not run while the object is made
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property bool ran: false
        z: NaN
        Text {
          width: root.z
          onWidthChanged: { root.ran = true; other.horizontalAlignment = ("number" * 1) }
          Text { id: other }
        }
      }
      """
    Then the property "ran" of "root" is false
    And the log has no warning matching "assign"

  # Found by ljos-fuzz (geometry), bucket value:load-handlers-positioner:number: the width stays
  # 0, so the Column leaves the child out whatever height it is then given.
  Scenario: A positioner's child whose width was given NaN stays out of its size
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        Column {
          id: column
          Item {
            id: child
            width: ("ab" * 1)
            Component.onCompleted: child.height = 100
          }
        }
        property real settled: column.implicitHeight
      }
      """
    When 50 milliseconds pass
    Then the property "settled" of "root" is 0

  # Found by ljos-fuzz (geometry), bucket value:load-anchors:nan-ljos.
  Scenario Outline: Anchors with a margin of NaN, <anchors>, read <seen>
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        width: 100
        height: 100
        property string seen
        Rectangle {
          id: box
          <anchors>
        }
        Component.onCompleted: seen = box.x + " " + box.y + " " + box.width + " " + box.height
      }
      """
    Then the property "seen" of "root" is "<seen>"

    Examples:
      | anchors                                                                 | seen            |
      | anchors.top: parent.verticalCenter; anchors.margins: NaN                | 0 0 0 0         |
      | anchors.top: parent.verticalCenter; anchors.topMargin: 3; anchors.margins: NaN | 0 53 0 0 |
      | anchors.left: parent.left; anchors.right: parent.right; anchors.margins: NaN | 0 0 0 0    |
      | anchors.fill: parent; anchors.margins: NaN                              | NaN NaN NaN NaN |

  # Found by ljos-fuzz (geometry), bucket value:load-free:nan-qt.
  Scenario: A size that follows its implicit size follows it to NaN
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        implicitWidth: parseFloat("")
        property bool nan: isNaN(width)
      }
      """
    Then the property "nan" of "root" is true

  # Found by ljos-fuzz (geometry), bucket value:load-free:nan-qt.
  Scenario: A size whose binding takes it back follows its implicit size to NaN
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        width: "#000000".a
        implicitWidth: parseFloat("")
        property bool nan: isNaN(width)
      }
      """
    Then the property "nan" of "root" is true

  # Found by ljos-fuzz (geometry), bucket value:load-handlers-center-in:nan-qt.
  Scenario: An item centred in its parent given an offset of NaN afterwards is at NaN
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property bool nan: isNaN(box.y)
        Rectangle {
          id: box
          anchors.centerIn: parent
          Component.onCompleted: box.anchors.verticalCenterOffset = NaN
        }
      }
      """
    Then the property "nan" of "root" is true

  # Found by ljos-fuzz (geometry), bucket value:load-center-in:nan-ljos: Qt rounds the centre to a
  # whole number first, and NaN comes out as the lowest int.
  @backlog
  Scenario: An item centred in a parent of height NaN is at the lowest int
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        implicitHeight: parseFloat("")
        Item {
          id: box
          anchors.centerIn: parent
        }
        property real at: box.y
      }
      """
    Then the property "at" of "root" is -2147483648

  # Found by ljos-fuzz (geometry), bucket value:load-handlers-free:number: a binding that gives
  # NaN gives nothing, so the size goes on following its implicit size.
  @backlog
  Scenario: A size given only NaN goes on following its implicit size
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: box
        objectName: "root"
        height: ("x" * 1)
        Component.onCompleted: box.implicitHeight = 4
      }
      """
    Then the property "height" of "root" is 4
