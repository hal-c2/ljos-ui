# Sources:
#   https://doc.qt.io/qt-6/qtqml-typesystem-valuetypes.html
#   https://doc.qt.io/qt-6/qml-point.html
#
# Qt 6.11.2 gives "QPointF(8, 9)" in the first scenario, "QPointF(5, 2),QPointF(5, 2)" in the
# second and "QPointF(6, 2),QPointF(6, 2),QPointF(6, 2)" in the third. Two values that hold the
# same being === is what Qt has and what scripts rely on; with that, a value has nowhere to keep
# which property it came from. A variable whose members the script writes keeps that beside it
# (value-types-in-variables.feature); these are the uses that do not.

@qt-diverges
Feature: A value type in a variable that is only read
  A point, size, rect, vector or font kept in a variable that the script does not write a member
  of is the value, as a number is. In Qt it is a reference to where the value was read from.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property point spot: Qt.point(1, 2)
        property string out
        function now() { const p = spot; spot = Qt.point(8, 9); out = "" + p }
        function bump(p) { p.x = 5; return p }
        function argument() { const q = bump(spot); out = q + "," + spot }
        function alias() { const a = spot; const b = a; b.x = 6; out = a + "," + b + "," + spot }
      }
      """

  # In Qt the constant still names the property and reads what it holds now.
  Scenario: A variable holds the value it was given, not the property
    When the method "now" of "root" is called
    Then the property "out" of "root" is "QPointF(1, 2)"

  # In Qt the parameter names the property it was given, and the write goes back to it.
  Scenario: Writing a member of a parameter changes the parameter alone
    When the method "argument" of "root" is called
    Then the property "out" of "root" is "QPointF(5, 2),QPointF(1, 2)"

  # In Qt both constants name the property, and all three are written.
  Scenario: A variable given another variable's value writes neither
    When the method "alias" of "root" is called
    Then the property "out" of "root" is "QPointF(1, 2),QPointF(6, 2),QPointF(1, 2)"
