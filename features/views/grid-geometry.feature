# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-gridview.html (cellWidth, cellHeight, flow, layoutDirection,
#   verticalLayoutDirection, header, footer)
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @api-QtQuick.GridView.cellWidth @api-QtQuick.GridView.flow
Feature: Where a GridView puts its cells
  The rows of the model fill cells of one size, as many to a line as fit in the view.

  Background:
    Given the QML:
      """qml
      import QtQuick
      GridView {
        objectName: "grid"
        width: 250; height: 200
        cellWidth: 80; cellHeight: 60
        model: 100
        delegate: Item { objectName: "g" + index; width: 70; height: 50 }
      }
      """

  Scenario: As many cells to a line as fit, then the next line
    Then the property "x" of "g1" is 80
    And the property "x" of "g2" is 160
    And the property "y" of "g2" is 0
    And the property "x" of "g3" is 0
    And the property "y" of "g3" is 60
    And the property "count" of "grid" is 100
    And the property "contentHeight" of "grid" is 2040

  Scenario: A narrower view has fewer cells to a line
    When the property "width" of "grid" is set to 170
    And 100 milliseconds pass
    Then the property "x" of "g2" is 0
    And the property "y" of "g2" is 60
    And the property "contentHeight" of "grid" is 3000

  Scenario: Another cell size lays the cells out again
    When the property "cellWidth" of "grid" is set to 125
    And 100 milliseconds pass
    Then the property "x" of "g1" is 125
    And the property "x" of "g2" is 0
    And the property "y" of "g2" is 60

  Scenario: Flowing from top to bottom fills columns and scrolls sideways
    When the property "flow" of "grid" is set to 1
    And 100 milliseconds pass
    Then the property "x" of "g1" is 0
    And the property "y" of "g1" is 60
    And the property "y" of "g2" is 120
    And the property "x" of "g3" is 80
    And the property "y" of "g3" is 0
    And the property "contentWidth" of "grid" is 2720

  Scenario: From right to left the first cell is at the right
    When the property "layoutDirection" of "grid" is set to 1
    And 100 milliseconds pass
    Then the property "x" of "g0" is 160
    And the property "x" of "g1" is 80
    And the property "x" of "g2" is 0
    And the property "x" of "g3" is 160
    And the property "contentX" of "grid" is -10

  Scenario: From bottom to top the first line is at the bottom
    When the property "verticalLayoutDirection" of "grid" is set to 3
    And 100 milliseconds pass
    Then the property "y" of "g0" is -60
    And the property "x" of "g1" is 80
    And the property "y" of "g3" is -120
    And the property "contentY" of "grid" is -200

  Scenario Outline: The defaults of a GridView
    Then the property "<property>" of "grid" is <value>

    Examples:
      | property                    | value |
      | flow                        | 0     |
      | cacheBuffer                 | 320   |
      | currentIndex                | 0     |
      | reuseItems                  | false |
      | keyNavigationEnabled        | true  |
      | keyNavigationWraps          | false |
      | highlightFollowsCurrentItem | true  |
      | highlightMoveDuration       | 150   |
      | snapMode                    | 0     |

  Scenario: Cells are a hundred pixels wide and high unless told otherwise
    Given the QML:
      """qml
      import QtQuick
      GridView {
        objectName: "plain"
        width: 250; height: 200
        model: 7
        delegate: Item { objectName: "p" + index; width: 10; height: 10 }
      }
      """
    Then the property "cellWidth" of "plain" is 100
    And the property "cellHeight" of "plain" is 100
    And the property "x" of "p1" is 100
    And the property "y" of "p2" is 100
    And the property "contentHeight" of "plain" is 400
