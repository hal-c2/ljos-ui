# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-layouts-rowlayout.html
#   https://doc.qt.io/qt-6/qml-qtquick-layouts-layout.html
#   https://doc.qt.io/qt-6/qtquicklayouts-overview.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: RowLayout
  A RowLayout gives every visible child a cell in a row. Each child has a
  minimum, a preferred and a maximum width; the row hands out its own width
  between them.

  Rule: Preferred sizes

    Scenario Outline: The preferred width is Layout.preferredWidth, else the implicit width, else the width
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Layouts
        RowLayout {
          objectName: "row"
          Item { objectName: "a"; Layout.preferredWidth: 30; implicitWidth: 50; width: 70; height: 10 }
          Item { objectName: "b"; implicitWidth: 50; width: 70; height: 10 }
          Item { objectName: "c"; width: 70; height: 10 }
        }
        """
      When 100 milliseconds pass
      Then the property "x" of "<item>" is <x>
      And the property "width" of "<item>" is <width>

      Examples:
        | item | x  | width |
        | a    | 0  | 30    |
        | b    | 35 | 50    |
        | c    | 90 | 70    |

    Scenario: A layout without a size takes its implicit size from its children and spacing
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Layouts
        RowLayout {
          objectName: "row"
          spacing: 4
          Item { implicitWidth: 30; implicitHeight: 12 }
          Item { implicitWidth: 50; implicitHeight: 20 }
        }
        """
      When 100 milliseconds pass
      Then the property "implicitWidth" of "row" is 84
      And the property "implicitHeight" of "row" is 20
      And the property "width" of "row" is 84
      And the property "height" of "row" is 20

    Scenario Outline: A zero preferred width is kept, and an implicit width is rounded up
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Layouts
        RowLayout {
          objectName: "row"
          Item { objectName: "a"; width: 50; height: 20 }
          Item { objectName: "none"; height: 20 }
          Item { objectName: "zero"; implicitWidth: 50; height: 20; Layout.preferredWidth: 0 }
          Item { objectName: "part"; implicitWidth: 10.4; height: 20 }
          Item { objectName: "last"; width: 20; height: 20 }
        }
        """
      When 100 milliseconds pass
      Then the property "x" of "<item>" is <x>
      And the property "width" of "<item>" is <width>

      Examples:
        | item | x  | width |
        | none | 55 | 0     |
        | zero | 60 | 0     |
        | part | 65 | 11    |
        | last | 81 | 20    |

    Scenario Outline: Minimum and maximum widths bound the preferred width, and the maximum wins
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Layouts
        RowLayout {
          objectName: "row"
          Item { objectName: "a"; height: 20; Layout.preferredWidth: 50; Layout.minimumWidth: 80 }
          Item { objectName: "b"; height: 20; Layout.preferredWidth: 30; Layout.maximumWidth: 20 }
          Item {
            objectName: "c"; height: 20
            Layout.preferredWidth: 30; Layout.minimumWidth: 40; Layout.maximumWidth: 35
          }
        }
        """
      When 100 milliseconds pass
      Then the property "width" of "<item>" is <width>

      Examples:
        | item | width |
        | a    | 80    |
        | b    | 20    |
        | c    | 35    |

  Rule: More room than the children prefer

    Scenario: Filling children share the extra width in proportion to their preferred widths
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Layouts
        RowLayout {
          objectName: "row"
          width: 400; height: 20
          Item { objectName: "a"; implicitWidth: 100; Layout.fillWidth: true }
          Item { objectName: "b"; implicitWidth: 50; Layout.fillWidth: true }
        }
        """
      When 100 milliseconds pass
      Then the property "width" of "a" is 263
      And the property "x" of "b" is 268
      And the property "width" of "b" is 132

    Scenario: A filling child with no preferred width takes what the others leave
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Layouts
        RowLayout {
          objectName: "row"
          width: 300; height: 20
          Item { objectName: "a"; width: 50; height: 20 }
          Item { objectName: "b"; height: 20; Layout.fillWidth: true }
        }
        """
      When 100 milliseconds pass
      Then the property "width" of "a" is 50
      And the property "x" of "b" is 55
      And the property "width" of "b" is 245

    Scenario: A filling child stops at its maximum width and the others take the rest
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Layouts
        RowLayout {
          objectName: "row"
          width: 300; height: 20
          Item { objectName: "a"; implicitWidth: 50; Layout.fillWidth: true; Layout.maximumWidth: 80 }
          Item { objectName: "b"; implicitWidth: 50; Layout.fillWidth: true }
        }
        """
      When 100 milliseconds pass
      Then the property "width" of "a" is 80
      And the property "x" of "b" is 85
      And the property "width" of "b" is 215

    Scenario: When no child can grow the cells grow and the children keep their size
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Layouts
        RowLayout {
          objectName: "row"
          width: 300; height: 20
          Item { objectName: "a"; width: 50; height: 20 }
          Item { objectName: "b"; width: 30; height: 20 }
        }
        """
      When 100 milliseconds pass
      Then the property "width" of "a" is 50
      And the property "x" of "b" is 189
      And the property "width" of "b" is 30

  Rule: Less room than the children prefer

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Layouts
        RowLayout {
          objectName: "row"
          width: 100; height: 20; spacing: 0
          Item { objectName: "a"; implicitWidth: 100; Layout.fillWidth: true }
          Item { objectName: "b"; implicitWidth: 50; Layout.fillWidth: true }
        }
        """
      When 100 milliseconds pass

    Scenario Outline: Filling children shrink towards their minimum width, the larger one faster
      When the property "width" of "row" is set to <width>
      And 100 milliseconds pass
      Then the property "width" of "a" is <a>
      And the property "width" of "b" is <b>

      Examples:
        | width | a  | b  |
        | 120   | 78 | 42 |
        | 100   | 64 | 36 |
        | 60    | 36 | 24 |
        | 20    | 10 | 10 |

    Scenario: Spacing is taken off before the children shrink
      When the property "spacing" of "row" is set to 5
      And 100 milliseconds pass
      Then the property "width" of "a" is 60
      And the property "x" of "b" is 65
      And the property "width" of "b" is 35

  Rule: Minimum sizes and fixed children when room is short

    Scenario Outline: A minimum width is kept while the others shrink
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Layouts
        RowLayout {
          width: 150; height: 20; spacing: 0
          Item { objectName: "a"; implicitWidth: 100; Layout.fillWidth: true; Layout.minimumWidth: 80 }
          Item { objectName: "b"; implicitWidth: 50; Layout.fillWidth: true }
          Item { objectName: "c"; implicitWidth: 60; Layout.fillWidth: true }
        }
        """
      When 100 milliseconds pass
      Then the property "width" of "<item>" is <width>

      Examples:
        | item | width |
        | a    | 92    |
        | b    | 27    |
        | c    | 31    |

    Scenario: A child that does not fill keeps its size and the filling one gives way
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Layouts
        RowLayout {
          width: 100; height: 20
          Item { objectName: "a"; width: 100; height: 20 }
          Item { objectName: "b"; implicitWidth: 50; height: 20; Layout.fillWidth: true }
        }
        """
      When 100 milliseconds pass
      Then the property "width" of "a" is 100
      And the property "x" of "b" is 105
      And the property "width" of "b" is 0

    Scenario: Below the sum of the minimum widths the children overflow the layout
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Layouts
        RowLayout {
          width: 50; height: 20
          Item { objectName: "a"; implicitWidth: 100; Layout.fillWidth: true; Layout.minimumWidth: 80 }
          Item { objectName: "b"; implicitWidth: 50; Layout.fillWidth: true; Layout.minimumWidth: 40 }
        }
        """
      When 100 milliseconds pass
      Then the property "width" of "a" is 80
      And the property "x" of "b" is 85
      And the property "width" of "b" is 40

  Rule: Which children take part

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Layouts
        RowLayout {
          objectName: "row"
          Item { objectName: "a"; width: 50; height: 20 }
          Item { objectName: "b"; width: 30; height: 20; visible: false }
          Item { objectName: "c"; width: 20; height: 20; property real want: -1; Layout.preferredWidth: want }
        }
        """
      When 100 milliseconds pass

    Scenario: An invisible child has no cell and no spacing
      Then the property "x" of "c" is 55
      And the property "implicitWidth" of "row" is 75

    Scenario: A child that becomes visible gets a cell
      When the property "visible" of "b" is set to true
      And 100 milliseconds pass
      Then the property "x" of "b" is 55
      And the property "x" of "c" is 90
      And the property "width" of "row" is 110

    Scenario: A new preferred width lays the row out again
      When the property "want" of "c" is set to 60
      And 100 milliseconds pass
      Then the property "width" of "c" is 60
      And the property "width" of "row" is 115

    Scenario: A child's own width is not watched once the layout has taken it
      When the property "width" of "a" is set to 80
      And 100 milliseconds pass
      Then the property "width" of "a" is 80
      And the property "x" of "c" is 55
      And the property "implicitWidth" of "row" is 75
