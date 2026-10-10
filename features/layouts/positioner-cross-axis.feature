# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-row.html
#   https://doc.qt.io/qt-6/qml-qtquick-column.html
#   https://doc.qt.io/qt-6/qtquick-positioning-layouts.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Row and Column across their direction
  A Row places its children along x and a Column along y. The other
  coordinate stays the child's own: the positioner only moves it by its
  padding on that side, and by the difference when the padding changes.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        width: 400; height: 200
        Column {
          objectName: "column"
          Rectangle { objectName: "a"; x: 7; width: 10; height: 10 }
          Rectangle { objectName: "b"; width: 10; height: 10 }
          Rectangle { objectName: "late"; x: 3; width: 10; height: 10; visible: false }
        }
        Row {
          objectName: "row"
          y: 100
          Rectangle { objectName: "c"; y: 7; width: 10; height: 10 }
          Rectangle { objectName: "d"; width: 10; height: 10 }
        }
      }
      """
    When 100 milliseconds pass

  Scenario: A Column leaves x to the child and a Row leaves y
    Then the property "x" of "a" is 7
    And the property "y" of "a" is 0
    And the property "y" of "b" is 10
    And the property "y" of "c" is 7
    And the property "x" of "c" is 0
    And the property "x" of "d" is 10

  Scenario: Padding moves a child that far from where it was
    When the property "leftPadding" of "column" is set to 5
    And the property "topPadding" of "row" is set to 5
    And 100 milliseconds pass
    Then the property "x" of "a" is 12
    And the property "x" of "b" is 5
    And the property "y" of "c" is 12
    And the property "y" of "d" is 5

  Scenario: A change of padding moves a child by the difference
    When the property "leftPadding" of "column" is set to 5
    And 100 milliseconds pass
    And the property "leftPadding" of "column" is set to 8
    And 100 milliseconds pass
    Then the property "x" of "a" is 15
    And the property "x" of "b" is 8

  Scenario: A position given in between is moved by the next difference only
    When the property "leftPadding" of "column" is set to 8
    And 100 milliseconds pass
    And the property "x" of "b" is set to 40
    And 100 milliseconds pass
    Then the property "x" of "b" is 40
    When the property "leftPadding" of "column" is set to 10
    And 100 milliseconds pass
    Then the property "x" of "b" is 42
    And the property "x" of "a" is 17

  Scenario: A child that is positioned for the first time moves by the whole padding
    When the property "leftPadding" of "column" is set to 8
    And 100 milliseconds pass
    Then the property "x" of "late" is 3
    When the property "visible" of "late" is set to true
    And 100 milliseconds pass
    Then the property "x" of "late" is 11
    And the property "y" of "late" is 20

  Scenario: The padding of all sides counts where the side has none of its own
    When the property "padding" of "column" is set to 20
    And the property "padding" of "row" is set to 4
    And 100 milliseconds pass
    Then the property "x" of "a" is 27
    And the property "y" of "a" is 20
    And the property "y" of "c" is 11
    And the property "x" of "c" is 4
