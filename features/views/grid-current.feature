# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-gridview.html (currentIndex, highlight, moveCurrentIndexUp and
#   friends, positionViewAtIndex, indexAt, header, footer, GridView.view, GridView.isCurrentItem)
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @api-QtQuick.GridView.currentIndex @api-QtQuick.GridView.positionViewAtIndex
Feature: The current cell of a GridView and where the view is
  A grid has a current cell, a highlight, and the same ways of moving and asking as a list.

  Background:
    Given the QML:
      """qml
      import QtQuick
      GridView {
        id: grid
        objectName: "grid"
        width: 250; height: 200
        cellWidth: 80; cellHeight: 60
        focus: true
        model: 100
        property int found: -2
        function ask(x, y) { found = grid.indexAt(x, y) }
        highlight: Rectangle { objectName: "mark"; color: "#ffcc00" }
        delegate: Item {
          objectName: "g" + index; width: 70; height: 50
          property bool current: GridView.isCurrentItem
          property string view: GridView.view.objectName
        }
      }
      """

  Scenario: The first cell is current and the highlight has the size of its delegate
    Then the property "currentIndex" of "grid" is 0
    And the property "currentItem.objectName" of "grid" is "g0"
    And the property "current" of "g0" is true
    And the property "view" of "g4" is "grid"
    And the property "width" of "mark" is 70
    And the property "height" of "mark" is 50

  Scenario Outline: Moving the current cell
    When the property "currentIndex" of "grid" is set to <from>
    And the method "<method>" of "grid" is called
    Then the property "currentIndex" of "grid" is <to>

    Examples:
      | from | method                | to |
      | 4    | moveCurrentIndexDown  | 7  |
      | 7    | moveCurrentIndexRight | 8  |
      | 8    | moveCurrentIndexUp    | 5  |
      | 5    | moveCurrentIndexLeft  | 4  |
      | 0    | moveCurrentIndexLeft  | 0  |
      | 1    | moveCurrentIndexUp    | 1  |
      | 2    | moveCurrentIndexRight | 3  |
      | 99   | moveCurrentIndexRight | 99 |
      | 98   | moveCurrentIndexDown  | 98 |

  Scenario: The arrow keys move the current cell
    When the user presses "Right"
    And the user presses "Down"
    Then the property "currentIndex" of "grid" is 4
    When the user presses "Left"
    And the user presses "Up"
    Then the property "currentIndex" of "grid" is 0

  Scenario: The highlight moves to the current cell
    When the property "currentIndex" of "grid" is set to 4
    And 1 second passes
    Then the property "x" of "mark" is 80
    And the property "y" of "mark" is 60

  Scenario: The view scrolls to a current cell outside it
    When the property "currentIndex" of "grid" is set to 50
    And 2 seconds pass
    Then the property "contentY" of "grid" is 820
    And the property "y" of "mark" is 960

  Scenario Outline: A cell is put at a place of the view
    When the method "positionViewAtIndex" of "grid" is called with 50, <mode>
    Then the property "contentY" of "grid" is <contentY>

    Examples:
      | mode | contentY | what      |
      | 0    | 960      | Beginning |
      | 1    | 890      | Center    |
      | 2    | 820      | End       |

  Scenario Outline: Which cell is at a place of the content
    When the method "ask" of "grid" is called with <x>, <y>
    Then the property "found" of "grid" is <index>

    Examples:
      | x   | y  | index |
      | 85  | 5  | 1     |
      | 85  | 65 | 4     |
      | 75  | 5  | 0     |
      | 245 | 5  | -1    |
