# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-layouts-layout.html
#   https://doc.qt.io/qt-6/qml-qtquick-layouts-rowlayout.html
#   https://doc.qt.io/qt-6/qtquicklayouts-overview.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Alignment, margins and the other axis of a RowLayout
  A child smaller than its cell is placed by Layout.alignment. Margins are
  part of the cell. Across the row a child keeps its preferred height unless
  it fills.

  Scenario Outline: A child smaller than its cell is aligned, by default left and vertically centred
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Layouts
      RowLayout {
        width: 300; height: 60
        Item { objectName: "a"; width: 50; height: 20; Layout.alignment: Qt.AlignRight }
        Item { objectName: "b"; width: 50; height: 20; Layout.alignment: Qt.AlignHCenter | Qt.AlignTop }
        Item { objectName: "c"; width: 50; height: 20; Layout.alignment: Qt.AlignLeft | Qt.AlignBottom }
      }
      """
    When 100 milliseconds pass
    Then the property "x" of "<item>" is <x>
    And the property "y" of "<item>" is <y>
    And the property "width" of "<item>" is 50

    Examples:
      | item | x   | y  |
      | a    | 47  | 20 |
      | b    | 125 | 0  |
      | c    | 203 | 40 |

  Scenario Outline: Across the row a child fills up to its maximum height or keeps its preferred height
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Layouts
      RowLayout {
        objectName: "row"
        height: 61
        Item { objectName: "fill"; width: 10; Layout.fillHeight: true }
        Item {
          objectName: "capped"; width: 10
          Layout.fillHeight: true; Layout.maximumHeight: 30; Layout.alignment: Qt.AlignBottom
        }
        Item { objectName: "top"; width: 10; height: 20; Layout.alignment: Qt.AlignTop }
        Item { objectName: "bottom"; width: 10; height: 20; Layout.alignment: Qt.AlignBottom }
        Item { objectName: "middle"; width: 10; height: 20 }
      }
      """
    When 100 milliseconds pass
    Then the property "y" of "<item>" is <y>
    And the property "height" of "<item>" is <height>

    Examples:
      | item   | y  | height |
      | fill   | 0  | 61     |
      | capped | 31 | 30     |
      | top    | 0  | 20     |
      | bottom | 41 | 20     |
      | middle | 21 | 20     |

  Scenario Outline: Margins are part of a child's cell
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Layouts
      RowLayout {
        objectName: "row"
        height: 60
        Item { objectName: "a"; width: 50; height: 20; Layout.margins: 10 }
        Item { objectName: "b"; width: 50; height: 20; Layout.leftMargin: 3; Layout.topMargin: 20 }
        Item {
          objectName: "c"; width: 50
          Layout.margins: 4; Layout.rightMargin: 7; Layout.fillHeight: true
        }
      }
      """
    When 100 milliseconds pass
    Then the property "x" of "<item>" is <x>
    And the property "y" of "<item>" is <y>
    And the property "height" of "<item>" is <height>
    And the property "width" of "row" is 194
    And the property "implicitHeight" of "row" is 40

    Examples:
      | item | x   | y  | height |
      | a    | 10  | 20 | 20     |
      | b    | 78  | 30 | 20     |
      | c    | 137 | 4  | 52     |

  Scenario Outline: A right-to-left row mirrors the cells and the alignment, not the margins
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Layouts
      RowLayout {
        width: 300; height: 60
        layoutDirection: Qt.RightToLeft
        Item { objectName: "a"; width: 50; height: 20 }
        Item {
          objectName: "b"; implicitWidth: 50; height: 20
          Layout.fillWidth: true; Layout.maximumWidth: 100; Layout.leftMargin: 7
        }
        Item { objectName: "c"; width: 20; height: 20; Layout.alignment: Qt.AlignRight }
      }
      """
    When 100 milliseconds pass
    Then the property "x" of "<item>" is <x>
    And the property "width" of "<item>" is <width>

    Examples:
      | item | x   | width |
      | a    | 250 | 50    |
      | b    | 113 | 100   |
      | c    | 0   | 20    |

  Scenario: A row shorter than its tallest child lays out at that child's height
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Layouts
      RowLayout {
        height: 10
        Item { objectName: "tall"; width: 10; height: 41 }
        Item { objectName: "fill"; width: 10; Layout.fillHeight: true }
      }
      """
    When 100 milliseconds pass
    Then the property "height" of "tall" is 41
    And the property "y" of "tall" is 0
    And the property "height" of "fill" is 41
