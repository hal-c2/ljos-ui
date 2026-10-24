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
