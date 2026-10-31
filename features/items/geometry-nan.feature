# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#x-prop
# Verified on Qt 6.11.2 (qml6, offscreen).
# Found by ljos-fuzz (bindings), buckets value:bool:isNaN, value:double:default,
# value:double:ref, value:var(number):ref and value:point:step-set(Qt.point).

@qt-parity
Feature: An item's geometry is never NaN
  `x`, `y`, `width` and `height` given NaN keep the value they had: nothing changes and no
  change is told, whether a binding or a script gives it.

  @backlog
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

  @backlog
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
  @backlog
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
  @backlog
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
