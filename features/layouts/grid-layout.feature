# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-layouts-gridlayout.html
#   https://doc.qt.io/qt-6/qml-qtquick-layouts-layout.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: GridLayout
  A GridLayout puts its children in the cells of a grid. A column is as wide
  as its widest child and a row as tall as its tallest.

  Rule: Children take the next free cell in flow order

    Scenario Outline: With columns set the grid fills row by row
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Layouts
        GridLayout {
          objectName: "grid"
          columns: 3
          Item { objectName: "a"; width: 40; height: 10 }
          Item { objectName: "b"; width: 20; height: 20 }
          Item { objectName: "c"; width: 30; height: 12 }
          Item { objectName: "d"; width: 10; height: 8 }
          Item { objectName: "e"; width: 20; height: 30 }
          Item { objectName: "f"; width: 30; height: 6 }
          Item { objectName: "g"; width: 10; height: 7 }
        }
        """
      When 100 milliseconds pass
      Then the property "x" of "<item>" is <x>
      And the property "y" of "<item>" is <y>
      And the property "width" of "grid" is 100
      And the property "height" of "grid" is 67

      Examples:
        | item | x  | y  |
        | a    | 0  | 5  |
        | b    | 45 | 0  |
        | c    | 70 | 4  |
        | d    | 0  | 36 |
        | e    | 45 | 25 |
        | f    | 70 | 37 |
        | g    | 0  | 60 |

    Scenario Outline: With rows set and a top-to-bottom flow the grid fills column by column
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Layouts
        GridLayout {
          objectName: "grid"
          rows: 3; flow: GridLayout.TopToBottom; rowSpacing: 2; columnSpacing: 3
          Item { objectName: "a"; width: 40; height: 10 }
          Item { objectName: "b"; width: 20; height: 20 }
          Item { objectName: "c"; width: 30; height: 12 }
          Item { objectName: "d"; width: 10; height: 8 }
          Item { objectName: "e"; width: 20; height: 30 }
          Item { objectName: "f"; width: 30; height: 6 }
          Item { objectName: "g"; width: 10; height: 7 }
        }
        """
      When 100 milliseconds pass
      Then the property "x" of "<item>" is <x>
      And the property "y" of "<item>" is <y>
      And the property "width" of "grid" is 86
      And the property "height" of "grid" is 56

      Examples:
        | item | x  | y  |
        | a    | 0  | 0  |
        | b    | 0  | 17 |
        | c    | 0  | 44 |
        | d    | 43 | 1  |
        | e    | 43 | 12 |
        | f    | 43 | 47 |
        | g    | 76 | 2  |

    Scenario Outline: A grid with neither rows nor columns is one line along its flow
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Layouts
        Item {
          GridLayout {
            Item { objectName: "a1"; width: 40; height: 10 }
            Item { objectName: "a2"; width: 20; height: 20 }
          }
          GridLayout {
            flow: GridLayout.TopToBottom
            Item { objectName: "b1"; width: 40; height: 10 }
            Item { objectName: "b2"; width: 20; height: 20 }
          }
        }
        """
      When 100 milliseconds pass
      Then the property "x" of "<item>" is <x>
      And the property "y" of "<item>" is <y>

      Examples:
        | item | x  | y  |
        | a1   | 0  | 5  |
        | a2   | 45 | 0  |
        | b1   | 0  | 0  |
        | b2   | 0  | 15 |

    Scenario Outline: An invisible child takes no cell
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Layouts
        GridLayout {
          objectName: "grid"
          columns: 2
          Item { objectName: "a"; width: 10; height: 10 }
          Item { objectName: "hidden"; width: 90; height: 90; visible: false }
          Item { objectName: "b"; width: 10; height: 10 }
          Item { objectName: "c"; width: 10; height: 10 }
        }
        """
      When 100 milliseconds pass
      Then the property "x" of "<item>" is <x>
      And the property "y" of "<item>" is <y>
      And the property "implicitWidth" of "grid" is 25

      Examples:
        | item | x  | y  |
        | a    | 0  | 0  |
        | b    | 15 | 0  |
        | c    | 0  | 15 |

  Rule: A child can name its cell and span several

    Scenario Outline: Layout.row and Layout.column place a child, and the next one follows it
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Layouts
        GridLayout {
          objectName: "grid"
          columns: 3
          Item { objectName: "a"; width: 10; height: 10; Layout.row: 1; Layout.column: 1 }
          Item { objectName: "b"; width: 10; height: 10 }
          Item { objectName: "c"; width: 10; height: 10 }
          Item { objectName: "d"; width: 10; height: 10; Layout.column: 2 }
          Item { objectName: "e"; width: 10; height: 10 }
        }
        """
      When 100 milliseconds pass
      Then the property "x" of "<item>" is <x>
      And the property "y" of "<item>" is <y>

      Examples:
        | item | x  | y  |
        | a    | 15 | 15 |
        | b    | 30 | 15 |
        | c    | 0  | 30 |
        | d    | 30 | 0  |
        | e    | 0  | 15 |

    Scenario Outline: A child spanning columns widens them in proportion to their preferred widths
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Layouts
        GridLayout {
          objectName: "grid"
          columns: 2; columnSpacing: 4; rowSpacing: 1
          Item { objectName: "a"; width: 10; height: 10 }
          Item { objectName: "b"; width: 30; height: 10 }
          Item { objectName: "wide"; width: 100; height: 10; Layout.columnSpan: 2 }
          Item { objectName: "c"; width: 10; height: 10 }
          Item { objectName: "d"; width: 10; height: 10 }
        }
        """
      When 100 milliseconds pass
      Then the property "x" of "<item>" is <x>
      And the property "y" of "<item>" is <y>
      And the property "width" of "grid" is 100

      Examples:
        | item | x  | y  |
        | a    | 0  | 0  |
        | b    | 28 | 0  |
        | wide | 0  | 11 |
        | c    | 0  | 22 |
        | d    | 28 | 22 |

    Scenario Outline: A child spanning rows keeps the cells beside it free for the next children
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Layouts
        GridLayout {
          objectName: "grid"
          columns: 2; columnSpacing: 0; rowSpacing: 0
          Item { objectName: "tall"; width: 10; height: 50; Layout.rowSpan: 2 }
          Item { objectName: "a"; width: 10; height: 10 }
          Item { objectName: "b"; width: 10; height: 30 }
          Item { objectName: "c"; width: 10; height: 10 }
        }
        """
      When 100 milliseconds pass
      Then the property "x" of "<item>" is <x>
      And the property "y" of "<item>" is <y>

      Examples:
        | item | x  | y  |
        | tall | 0  | 0  |
        | a    | 10 | 2  |
        | b    | 10 | 17 |
        | c    | 0  | 50 |

  Rule: Columns and rows share the grid's size

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Layouts
        GridLayout {
          objectName: "grid"
          columns: 3; width: 300
          property bool wide: false
          Item { objectName: "a"; implicitWidth: 40; height: 10; Layout.fillWidth: parent.wide }
          Item { objectName: "b"; implicitWidth: 20; height: 10; Layout.fillWidth: true }
          Item { objectName: "c"; width: 30; height: 10 }
          Item { objectName: "d"; width: 10; height: 10 }
          Item { objectName: "e"; width: 10; height: 10; Layout.alignment: Qt.AlignRight }
          Item { objectName: "f"; width: 30; height: 10; Layout.fillWidth: true; Layout.maximumWidth: 20 }
        }
        """
      When 100 milliseconds pass

    Scenario: A column with a filling child takes the extra width
      Then the property "width" of "a" is 40
      And the property "x" of "b" is 45
      And the property "width" of "b" is 220
      And the property "x" of "c" is 270

    Scenario: A child that does not fill is aligned in its column
      Then the property "x" of "d" is 0
      And the property "x" of "e" is 255
      And the property "width" of "e" is 10

    Scenario: A filling child stops at its maximum width
      Then the property "x" of "f" is 270
      And the property "width" of "f" is 20

    Scenario: Columns that fill share the extra width in proportion to their preferred widths
      When the property "wide" of "grid" is set to true
      And 100 milliseconds pass
      Then the property "width" of "a" is 173
      And the property "x" of "b" is 178
      And the property "width" of "b" is 87
      And the property "x" of "c" is 270

  Rule: Direction and uniform cells

    Scenario Outline: A right-to-left grid mirrors its columns
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Layouts
        GridLayout {
          objectName: "grid"
          columns: 2; layoutDirection: Qt.RightToLeft
          Item { objectName: "a"; width: 40; height: 10 }
          Item { objectName: "b"; width: 20; height: 10 }
          Item { objectName: "c"; width: 10; height: 10 }
          Item { objectName: "d"; width: 10; height: 10; Layout.alignment: Qt.AlignRight }
        }
        """
      When 100 milliseconds pass
      Then the property "x" of "<item>" is <x>

      Examples:
        | item | x  |
        | a    | 25 |
        | b    | 0  |
        | c    | 55 |
        | d    | 0  |

    Scenario Outline: Uniform cell widths and heights make every column and row alike
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Layouts
        GridLayout {
          objectName: "grid"
          columns: 2; uniformCellWidths: true; uniformCellHeights: true
          Item { objectName: "a"; width: 40; height: 10 }
          Item { objectName: "b"; width: 20; height: 10 }
          Item { objectName: "c"; width: 10; height: 30 }
          Item { objectName: "d"; width: 10; height: 10 }
        }
        """
      When 100 milliseconds pass
      Then the property "x" of "<item>" is <x>
      And the property "y" of "<item>" is <y>
      And the property "width" of "grid" is 85
      And the property "height" of "grid" is 65

      Examples:
        | item | x  | y  |
        | a    | 0  | 10 |
        | b    | 45 | 10 |
        | c    | 0  | 35 |
        | d    | 45 | 45 |
