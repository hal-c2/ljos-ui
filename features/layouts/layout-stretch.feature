# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-layouts-layout.html
#   https://doc.qt.io/qt-6/qml-qtquick-layouts-rowlayout.html
#   https://doc.qt.io/qt-6/qml-qtquick-layouts-columnlayout.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Stretch factors and uniform cells
  A stretch factor says how much of the extra room a filling child takes.
  Uniform cell sizes give every child a cell of the same size.

  Rule: Stretch factors

    Scenario Outline: Two filling children of the same preferred width share the extra width by their stretch factors
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Layouts
        RowLayout {
          objectName: "row"
          width: 180; height: 20; spacing: 0
          property int first: <first>
          property int second: <second>
          Item {
            objectName: "a"; implicitWidth: 50
            Layout.fillWidth: true; Layout.horizontalStretchFactor: parent.first
          }
          Item {
            objectName: "b"; implicitWidth: 50
            Layout.fillWidth: true; Layout.horizontalStretchFactor: parent.second
          }
        }
        """
      When 100 milliseconds pass
      Then the property "width" of "a" is <a>
      And the property "width" of "b" is <b>

      Examples:
        | first | second | a  | b   |
        | 1     | 1      | 90 | 90  |
        | 1     | 2      | 66 | 114 |
        | 1     | 3      | 60 | 120 |
        | 2     | 3      | 73 | 107 |
        | 1     | -1     | 130 | 50 |
        | 0     | 2      | 50 | 130 |

    Scenario: A stretch factor does nothing for a child that does not fill
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Layouts
        RowLayout {
          width: 180; height: 20; spacing: 0
          Item { objectName: "a"; implicitWidth: 50; Layout.horizontalStretchFactor: 5 }
          Item { objectName: "b"; implicitWidth: 50; Layout.fillWidth: true }
        }
        """
      When 100 milliseconds pass
      Then the property "width" of "a" is 50
      And the property "width" of "b" is 130

    Scenario: Stretch factors do not change how children shrink
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Layouts
        RowLayout {
          width: 100; height: 20; spacing: 0
          Item {
            objectName: "a"; implicitWidth: 100
            Layout.fillWidth: true; Layout.horizontalStretchFactor: 1
          }
          Item {
            objectName: "b"; implicitWidth: 50
            Layout.fillWidth: true; Layout.horizontalStretchFactor: 4
          }
        }
        """
      When 100 milliseconds pass
      Then the property "width" of "a" is 64
      And the property "width" of "b" is 36

    Scenario: A ColumnLayout shares extra height by vertical stretch factors
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Layouts
        ColumnLayout {
          width: 20; height: 180; spacing: 0
          Item {
            objectName: "a"; implicitHeight: 50
            Layout.fillHeight: true; Layout.verticalStretchFactor: 1
          }
          Item {
            objectName: "b"; implicitHeight: 50
            Layout.fillHeight: true; Layout.verticalStretchFactor: 3
          }
        }
        """
      When 100 milliseconds pass
      Then the property "height" of "a" is 60
      And the property "height" of "b" is 120

  Rule: Uniform cell sizes

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Layouts
        RowLayout {
          objectName: "row"
          spacing: 0; uniformCellSizes: true
          Item { objectName: "a"; implicitWidth: 10; height: 20; Layout.fillWidth: true }
          Item { objectName: "b"; width: 20; height: 20 }
          Item { objectName: "c"; implicitWidth: 30; height: 20; Layout.fillWidth: true }
        }
        """
      When 100 milliseconds pass

    Scenario: Every cell prefers the size the children can all agree on
      Then the property "implicitWidth" of "row" is 60
      And the property "width" of "a" is 20
      And the property "x" of "b" is 20
      And the property "x" of "c" is 40
      And the property "width" of "c" is 20

    Scenario: Extra width goes to every cell alike
      When the property "width" of "row" is set to 300
      And 100 milliseconds pass
      Then the property "width" of "a" is 100
      And the property "x" of "b" is 100
      And the property "width" of "b" is 20
      And the property "x" of "c" is 200
      And the property "width" of "c" is 100
