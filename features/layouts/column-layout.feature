# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-layouts-columnlayout.html
#   https://doc.qt.io/qt-6/qml-qtquick-layouts-layout.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: ColumnLayout
  A ColumnLayout shares its height between its children the way a RowLayout
  shares its width.

  Scenario Outline: Filling children share the extra height and the others are aligned in their cells
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Layouts
      ColumnLayout {
        objectName: "col"
        width: 100; height: 300
        Item { objectName: "a"; width: 50; height: 20 }
        Item { objectName: "b"; width: 30; height: 40; Layout.fillHeight: true }
        Item { objectName: "c"; width: 30; height: 20; Layout.fillHeight: true; Layout.fillWidth: true }
        Item { objectName: "d"; width: 30; height: 20; Layout.alignment: Qt.AlignRight }
      }
      """
    When 100 milliseconds pass
    Then the property "x" of "<item>" is <x>
    And the property "y" of "<item>" is <y>
    And the property "width" of "<item>" is <width>
    And the property "height" of "<item>" is <height>

    Examples:
      | item | x  | y   | width | height |
      | a    | 0  | 0   | 50    | 20     |
      | b    | 0  | 25  | 30    | 163    |
      | c    | 0  | 193 | 100   | 82     |
      | d    | 70 | 280 | 30    | 20     |

  Scenario: A ColumnLayout without a size is as wide as its widest child and as tall as all of them
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Layouts
      ColumnLayout {
        objectName: "col"
        spacing: 3
        Item { implicitWidth: 30; implicitHeight: 12 }
        Item { implicitWidth: 50; implicitHeight: 20 }
        Item { implicitWidth: 90; implicitHeight: 20; visible: false }
      }
      """
    When 100 milliseconds pass
    Then the property "implicitWidth" of "col" is 50
    And the property "implicitHeight" of "col" is 35
    And the property "width" of "col" is 50
    And the property "height" of "col" is 35

  Scenario Outline: Filling children shrink when the column is short
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Layouts
      ColumnLayout {
        objectName: "col"
        width: 20; height: 100; spacing: 0
        Item { objectName: "a"; implicitHeight: 100; Layout.fillHeight: true }
        Item { objectName: "b"; implicitHeight: 50; Layout.fillHeight: true }
      }
      """
    When the property "height" of "col" is set to <height>
    And 100 milliseconds pass
    Then the property "height" of "a" is <a>
    And the property "y" of "b" is <a>
    And the property "height" of "b" is <b>

    Examples:
      | height | a   | b  |
      | 300    | 200 | 100 |
      | 120    | 78  | 42 |
      | 60     | 36  | 24 |
