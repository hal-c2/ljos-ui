# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-layouts-layout.html
#   https://doc.qt.io/qt-6/qtquicklayouts-overview.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A nested layout keeps the size it is told to prefer
  A layout inside a layout is no larger than its children let it be, unless
  it is told otherwise: an explicit Layout.preferredWidth or
  Layout.preferredHeight raises that bound, an explicit Layout.maximumWidth
  or Layout.maximumHeight still caps it, and the children's minimum still
  raises a smaller preferred size.

  Scenario Outline: A header row keeps its height whether its tall child shows or not
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Layouts
      Item {
        id: root
        objectName: "root"
        width: 200; height: 62
        property bool tall: false
        ColumnLayout {
          anchors.fill: parent
          spacing: 0
          RowLayout {
            objectName: "header"
            Layout.fillWidth: true
            Layout.preferredHeight: 22
            Rectangle { implicitWidth: 16; implicitHeight: 16 }
            Rectangle { visible: root.tall; implicitWidth: 22; implicitHeight: 22 }
          }
          Rectangle { objectName: "second"; Layout.fillWidth: true; implicitHeight: 18 }
          Rectangle { objectName: "third"; Layout.fillWidth: true; implicitHeight: 15 }
        }
      }
      """
    When the property "tall" of "root" is set to <tall>
    And 100 milliseconds pass
    Then the property "y" of "header" is 2
    And the property "height" of "header" is 22
    And the property "y" of "second" is 26
    And the property "y" of "third" is 46

    Examples:
      | tall  |
      | false |
      | true  |

  Scenario Outline: A layout in a column keeps the height it prefers
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Layouts
      <outer>
        width: 200; height: 62
        <inner> {
          objectName: "inner"
          Layout.fillWidth: true
          <attached>
          Rectangle { implicitWidth: 16; implicitHeight: 16 }
        }
        Rectangle { objectName: "second"; Layout.fillWidth: true; implicitHeight: 18 }
        Rectangle { Layout.fillWidth: true; implicitHeight: 15 }
      }
      """
    When 100 milliseconds pass
    Then the property "height" of "inner" is <height>
    And the property "y" of "second" is <second>

    Examples:
      | outer                                    | inner        | attached                                              | height | second |
      | ColumnLayout { spacing: 0                | RowLayout    | Layout.preferredHeight: 22                            | 22     | 26     |
      | ColumnLayout { spacing: 0                | ColumnLayout | Layout.preferredHeight: 22                            | 22     | 26     |
      | ColumnLayout { spacing: 0                | GridLayout   | Layout.preferredHeight: 22                            | 22     | 26     |
      | GridLayout { columns: 1; rowSpacing: 0   | RowLayout    | Layout.preferredHeight: 22                            | 22     | 26     |
      | ColumnLayout { spacing: 0                | RowLayout    | Layout.preferredHeight: 22; Layout.maximumHeight: 16  | 16     | 23     |
      | ColumnLayout { spacing: 0                | GridLayout   | Layout.preferredHeight: 10                            | 16     | 23     |
      | ColumnLayout { spacing: 0                | RowLayout    | Layout.preferredHeight: 22; Layout.fillHeight: true   | 22     | 26     |
      | ColumnLayout { spacing: 0                | ColumnLayout | Layout.preferredHeight: 22; Layout.fillHeight: false  | 22     | 26     |
      | ColumnLayout { spacing: 0                | RowLayout    | Layout.minimumHeight: 20                              | 20     | 25     |

  Scenario Outline: A layout in a row keeps the width it prefers
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Layouts
      <outer>
        width: 100; height: 20
        <inner> {
          objectName: "inner"
          Layout.fillHeight: true
          <attached>
          Rectangle { implicitWidth: 16; implicitHeight: 16 }
        }
        Rectangle { objectName: "second"; Layout.fillWidth: true; implicitWidth: 18 }
      }
      """
    When 100 milliseconds pass
    Then the property "width" of "inner" is <width>
    And the property "x" of "second" is <second>

    Examples:
      | outer                                    | inner        | attached                                            | width | second |
      | RowLayout { spacing: 0                   | RowLayout    | Layout.preferredWidth: 30                           | 30    | 30     |
      | RowLayout { spacing: 0                   | ColumnLayout | Layout.preferredWidth: 30                           | 30    | 30     |
      | RowLayout { spacing: 0                   | GridLayout   | Layout.preferredWidth: 30                           | 30    | 30     |
      | GridLayout { rows: 1; columnSpacing: 0   | ColumnLayout | Layout.preferredWidth: 30                           | 30    | 30     |
      | RowLayout { spacing: 0                   | RowLayout    | Layout.preferredWidth: 30; Layout.maximumWidth: 20  | 20    | 20     |
      | RowLayout { spacing: 0                   | ColumnLayout | Layout.preferredWidth: 10                           | 16    | 16     |
      | RowLayout { spacing: 0                   | GridLayout   | Layout.preferredWidth: 30; Layout.fillWidth: false  | 30    | 30     |
      | RowLayout { spacing: 0                   | RowLayout    | Layout.minimumWidth: 20                             | 20    | 20     |
