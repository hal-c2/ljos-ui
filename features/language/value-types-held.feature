# Sources:
#   https://doc.qt.io/qt-6/qtqml-typesystem-valuetypes.html
#   https://doc.qt.io/qt-6/qml-point.html
#
# Qt 6.11.2 gives "QPointF(5, 9)" and "QPointF(5, 9)" in the first scenario and "QPointF(5, 2)" in
# the second. Two values that hold the same being === is what Qt has and what scripts rely on;
# with that, a value has nowhere to keep which property or which constant it came from.

@qt-diverges
Feature: A value type in a variable
  A point, size, rect, vector or font kept in a variable is the value, as a number is. In Qt it
  is a reference to where the value was read from.

  # In Qt a variable that was given a property's value still names the property: it reads what
  # the property holds now, and writing its member writes the property. Here it holds the value
  # it was given, as a number or a string would.
  Scenario: A variable holds the value it was given, not the property
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property point spot: Qt.point(1, 2)
        property string kept
        property string text: "" + spot
        function change() {
          var copy = spot
          spot = Qt.point(8, 9)
          copy.x = 5
          kept = "" + copy
        }
      }
      """
    When the method "change" of "root" is called
    Then the property "kept" of "root" is "QPointF(5, 2)"
    And the property "text" of "root" is "QPointF(8, 9)"

  # Qt lets a member of a constant be written, because its constant names a place and not a
  # value. Here the value is the constant.
  Scenario: A member of a constant cannot be written
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string text: "untouched"
        function change() {
          const spot = Qt.point(1, 2)
          spot.x = 5
          text = "" + spot
        }
      }
      """
    When the method "change" of "root" is called
    Then the property "text" of "root" is "untouched"
    And the log has a warning matching "TypeError"
