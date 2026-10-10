# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-grid.html
#   https://doc.qt.io/qt-6/qtquick-positioning-layouts.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Grid
  A Grid places its children in cells. A column is as wide as its widest
  child and a row as tall as its tallest.

  Scenario Outline: Columns, spacing and a row spacing of its own
    Given the QML:
      """qml
      import QtQuick
      Grid {
        objectName: "grid"
        columns: 2; spacing: 3; rowSpacing: 7
        Item { objectName: "a"; width: 40; height: 10 }
        Item { objectName: "b"; width: 30; height: 20 }
        Item { objectName: "c"; width: 45; height: 25 }
        Item { objectName: "d"; width: 55; height: 30 }
        Item { objectName: "e"; width: 20; height: 5 }
      }
      """
    When 100 milliseconds pass
    Then the property "x" of "<item>" is <x>
    And the property "y" of "<item>" is <y>
    And the property "width" of "grid" is 103
    And the property "height" of "grid" is 69

    Examples:
      | item | x  | y  |
      | a    | 0  | 0  |
      | b    | 48 | 0  |
      | c    | 0  | 27 |
      | d    | 48 | 27 |
      | e    | 0  | 64 |

  Scenario Outline: A Grid has four columns unless told otherwise, and rows alone set the columns
    Given the QML:
      """qml
      import QtQuick
      Item {
        Grid {
          objectName: "four"
          Item { width: 10; height: 10 }
          Item { width: 10; height: 10 }
          Item { width: 10; height: 10 }
          Item { objectName: "f4"; width: 10; height: 10 }
          Item { objectName: "f5"; width: 10; height: 10 }
        }
        Grid {
          objectName: "rows"
          rows: 2
          Item { width: 10; height: 10 }
          Item { width: 10; height: 10 }
          Item { objectName: "r3"; width: 10; height: 10 }
          Item { objectName: "r4"; width: 10; height: 10 }
          Item { objectName: "r5"; width: 10; height: 10 }
        }
      }
      """
    When 100 milliseconds pass
    Then the property "x" of "<item>" is <x>
    And the property "y" of "<item>" is <y>

    Examples:
      | item | x  | y  |
      | f4   | 30 | 0  |
      | f5   | 0  | 10 |
      | r3   | 20 | 0  |
      | r4   | 0  | 10 |
      | r5   | 10 | 10 |

  Scenario Outline: A top-to-bottom Grid fills column by column
    Given the QML:
      """qml
      import QtQuick
      Grid {
        objectName: "grid"
        rows: 2; flow: Grid.TopToBottom; columnSpacing: 4
        Item { objectName: "a"; width: 40; height: 10 }
        Item { objectName: "b"; width: 30; height: 20 }
        Item { objectName: "c"; width: 45; height: 25 }
        Item { objectName: "hidden"; width: 90; height: 90; visible: false }
        Item { objectName: "d"; width: 55; height: 30 }
        Item { objectName: "e"; width: 20; height: 5 }
      }
      """
    When 100 milliseconds pass
    Then the property "x" of "<item>" is <x>
    And the property "y" of "<item>" is <y>
    And the property "width" of "grid" is 123
    And the property "height" of "grid" is 55

    Examples:
      | item | x   | y  |
      | a    | 0   | 0  |
      | b    | 0   | 25 |
      | c    | 44  | 0  |
      | d    | 44  | 25 |
      | e    | 103 | 0  |

  Scenario Outline: Children are aligned in their cells
    Given the QML:
      """qml
      import QtQuick
      Grid {
        objectName: "grid"
        columns: 2
        horizontalItemAlignment: Grid.AlignHCenter
        verticalItemAlignment: Grid.AlignBottom
        Item { objectName: "a"; width: 40; height: 10 }
        Item { objectName: "b"; width: 30; height: 20 }
        Item { objectName: "c"; width: 45; height: 25 }
        Item { objectName: "d"; width: 55; height: 31 }
      }
      """
    When 100 milliseconds pass
    Then the property "x" of "<item>" is <x>
    And the property "y" of "<item>" is <y>
    And the property "width" of "grid" is 100
    And the property "height" of "grid" is 51

    Examples:
      | item | x    | y  |
      | a    | 2.5  | 10 |
      | b    | 57.5 | 0  |
      | c    | 0    | 26 |
      | d    | 45   | 20 |

  Scenario Outline: Padding surrounds the cells
    Given the QML:
      """qml
      import QtQuick
      Grid {
        objectName: "grid"
        columns: 2
        padding: 3; rightPadding: 9; bottomPadding: 11
        horizontalItemAlignment: Grid.AlignRight
        verticalItemAlignment: Grid.AlignVCenter
        Item { objectName: "a"; width: 40; height: 10 }
        Item { objectName: "b"; width: 30; height: 20 }
        Item { objectName: "c"; width: 45; height: 25 }
        Item { objectName: "d"; width: 55; height: 31 }
      }
      """
    When 100 milliseconds pass
    Then the property "x" of "<item>" is <x>
    And the property "y" of "<item>" is <y>
    And the property "width" of "grid" is 112
    And the property "height" of "grid" is 65

    Examples:
      | item | x  | y  |
      | a    | 8  | 8  |
      | b    | 73 | 3  |
      | c    | 3  | 26 |
      | d    | 48 | 23 |

  Scenario Outline: A right-to-left Grid counts its columns from the right and flips the alignment
    Given the QML:
      """qml
      import QtQuick
      Grid {
        objectName: "grid"
        width: 200; columns: 2; layoutDirection: Qt.RightToLeft
        Item { objectName: "a"; width: 40; height: 20 }
        Item { objectName: "b"; width: 35; height: 20 }
        Item { objectName: "c"; width: 30; height: 20 }
      }
      """
    When 100 milliseconds pass
    Then the property "x" of "<item>" is <x>
    And the property "y" of "<item>" is <y>
    And the property "effectiveHorizontalItemAlignment" of "grid" is 2

    Examples:
      | item | x   | y  |
      | a    | 160 | 0  |
      | b    | 125 | 0  |
      | c    | 170 | 20 |

  Scenario: Children beyond the last cell are left where they are
    Given the QML:
      """qml
      import QtQuick
      Grid {
        objectName: "grid"
        rows: 1; columns: 2
        Item { objectName: "a"; width: 10; height: 10 }
        Item { objectName: "b"; width: 10; height: 10 }
        Item { objectName: "extra"; x: 77; y: 33; width: 10; height: 10 }
      }
      """
    When 100 milliseconds pass
    Then the property "x" of "b" is 10
    And the property "x" of "extra" is 77
    And the property "y" of "extra" is 33
    And the property "width" of "grid" is 20
