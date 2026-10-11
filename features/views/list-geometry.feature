# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-listview.html (orientation, spacing, layoutDirection, verticalLayoutDirection)
#   https://doc.qt.io/qt-6/qml-qtquick-flickable.html (contentHeight, originY)
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @api-QtQuick.ListView.orientation @api-QtQuick.ListView.spacing
Feature: Where a ListView puts its rows
  Rows follow one another along the view's orientation, and the content is as long as the rows.

  Background:
    Given the QML:
      """qml
      import QtQuick
      ListView {
        objectName: "list"
        width: 200; height: 300
        model: 8
        delegate: Item { objectName: "r" + index; width: 40; height: 20 }
      }
      """

  Scenario: Rows stack downwards
    Then the property "y" of "r0" is 0
    And the property "y" of "r3" is 60
    And the property "x" of "r3" is 0
    And the property "contentHeight" of "list" is 160
    And the property "contentY" of "list" is 0
    And the property "originY" of "list" is 0

  Scenario: Spacing goes between rows, not around them
    When the property "spacing" of "list" is set to 5
    And 100 milliseconds pass
    Then the property "y" of "r1" is 25
    And the property "y" of "r7" is 175
    And the property "contentHeight" of "list" is 195

  Scenario: A horizontal list puts rows side by side
    When the property "orientation" of "list" is set to 1
    And 100 milliseconds pass
    Then the property "x" of "r3" is 120
    And the property "y" of "r3" is 0
    And the property "contentWidth" of "list" is 320

  Scenario: A horizontal list from right to left starts at the right edge
    When the property "orientation" of "list" is set to 1
    And the property "layoutDirection" of "list" is set to 1
    And 100 milliseconds pass
    Then the property "x" of "r0" is -40
    And the property "x" of "r3" is -160
    And the property "contentX" of "list" is -200
    And the property "originX" of "list" is -320
    And the property "contentWidth" of "list" is 320

  Scenario: A list from bottom to top starts at the bottom edge
    When the property "verticalLayoutDirection" of "list" is set to 3
    And 100 milliseconds pass
    Then the property "y" of "r0" is -20
    And the property "y" of "r3" is -80
    And the property "contentY" of "list" is -300
    And the property "originY" of "list" is -160
    And the property "contentHeight" of "list" is 160

  Scenario: A row that changes its height moves the rows after it
    When the property "height" of "r2" is set to 50
    And 100 milliseconds pass
    Then the property "y" of "r2" is 40
    And the property "y" of "r3" is 90
    And the property "contentHeight" of "list" is 190

  Scenario Outline: The defaults of a ListView
    Then the property "<property>" of "list" is <value>

    Examples:
      | property                    | value |
      | orientation                 | 2     |
      | spacing                     | 0     |
      | cacheBuffer                 | 320   |
      | currentIndex                | 0     |
      | reuseItems                  | false |
      | keyNavigationEnabled        | true  |
      | keyNavigationWraps          | false |
      | highlightFollowsCurrentItem | true  |
      | highlightMoveVelocity       | 400   |
      | highlightMoveDuration       | -1    |
      | highlightRangeMode          | 0     |
      | snapMode                    | 0     |
      | displayMarginBeginning      | 0     |
      | displayMarginEnd            | 0     |
      | layoutDirection             | 0     |
      | effectiveLayoutDirection    | 0     |
      | verticalLayoutDirection     | 2     |
      | clip                        | false |
      | contentWidth                | -1    |
