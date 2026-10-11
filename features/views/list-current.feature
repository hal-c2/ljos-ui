# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-listview.html (currentIndex, currentItem, highlight, highlightItem,
#   highlightFollowsCurrentItem, highlightMoveDuration, incrementCurrentIndex, ListView.isCurrentItem)
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @api-QtQuick.ListView.currentIndex @api-QtQuick.ListView.highlight
Feature: The current row of a ListView and its highlight
  One row is current. The highlight is an item the view moves to the current row, and the view
  scrolls so that the current row can be seen.

  Background:
    Given the QML:
      """qml
      import QtQuick
      ListView {
        objectName: "list"
        width: 200; height: 300
        model: 100
        highlight: Rectangle { objectName: "mark"; color: "#ffcc00" }
        delegate: Item {
          objectName: "r" + index; width: 200; height: 20
          property bool current: ListView.isCurrentItem
        }
      }
      """

  Scenario: The first row is current at the start
    Then the property "currentIndex" of "list" is 0
    And the property "currentItem.objectName" of "list" is "r0"
    And the property "current" of "r0" is true
    And the property "current" of "r1" is false

  Scenario: The highlight has the place and the size of the current row
    Then the property "highlightItem.objectName" of "list" is "mark"
    And the property "y" of "mark" is 0
    And the property "width" of "mark" is 200
    And the property "height" of "mark" is 20
    And the property "z" of "mark" is 0

  Scenario: Another current row moves the highlight there
    When the property "currentIndex" of "list" is set to 5
    Then the property "currentItem.objectName" of "list" is "r5"
    And the property "current" of "r5" is true
    And the property "current" of "r0" is false
    When 1 second passes
    Then the property "y" of "mark" is 100
    And the property "contentY" of "list" is 0

  Scenario: The highlight takes time to get there
    When the property "currentIndex" of "list" is set to 10
    And 100 milliseconds pass
    Then the property "y" of "mark" is at least 1
    And the property "y" of "mark" is at most 100
    When 1 second passes
    Then the property "y" of "mark" is 200

  Scenario: The view scrolls until a current row below it is at its end
    When the property "currentIndex" of "list" is set to 30
    And 2 seconds pass
    Then the property "y" of "mark" is 600
    And the property "contentY" of "list" is 320

  Scenario: The view scrolls until a current row above it is at its start
    When the property "contentY" of "list" is set to 1000
    And the property "currentIndex" of "list" is set to 40
    And 2 seconds pass
    Then the property "contentY" of "list" is 800

  Scenario: With no move duration the highlight and the view jump
    When the property "highlightMoveDuration" of "list" is set to 0
    And the property "currentIndex" of "list" is set to 80
    Then the property "y" of "mark" is 1600
    And the property "contentY" of "list" is 1320

  Scenario: A highlight that does not follow stays where it is
    When the property "highlightFollowsCurrentItem" of "list" is set to false
    And the property "currentIndex" of "list" is set to 60
    And 1 second passes
    Then the property "y" of "mark" is 0
    And the property "contentY" of "list" is 0
    And the property "currentItem.objectName" of "list" is "r60"

  Scenario: No row is current at index -1
    When the property "currentIndex" of "list" is set to -1
    Then the property "currentItem" of "list" is null
    And the property "highlightItem" of "list" is null
    And the property "current" of "r0" is false
    And the object "mark" does not exist

  Scenario: Stepping stops at the last row
    When the property "currentIndex" of "list" is set to 98
    And the method "incrementCurrentIndex" of "list" is called
    And the method "incrementCurrentIndex" of "list" is called
    Then the property "currentIndex" of "list" is 99

  Scenario: Stepping back stops at the first row
    When the method "decrementCurrentIndex" of "list" is called
    Then the property "currentIndex" of "list" is 0

  Scenario: Stepping wraps around when key navigation wraps
    When the property "keyNavigationWraps" of "list" is set to true
    And the method "decrementCurrentIndex" of "list" is called
    Then the property "currentIndex" of "list" is 99
    When the method "incrementCurrentIndex" of "list" is called
    Then the property "currentIndex" of "list" is 0

  Scenario: The current row is kept when it scrolls out of the view
    When the property "contentY" of "list" is set to 1500
    And 100 milliseconds pass
    Then the object "r0" exists
    And the object "r5" does not exist
    And the property "currentItem.objectName" of "list" is "r0"
