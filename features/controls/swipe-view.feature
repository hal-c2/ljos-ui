# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-swipeview.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-container.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: SwipeView
  A swipe view shows one page at a time, side by side in a row (or a
  column), each as large as the view. A swipe that comes to rest on a
  page makes it the current one; setting the current page moves the view
  to it. Each page knows its place and whether it is the current one, or
  the one just before or after it.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        id: root; objectName: "root"; width: 300; height: 200
        property string seen: ""
        SwipeView {
          id: view; objectName: "view"; anchors.fill: parent
          Rectangle { id: a; objectName: "a"; color: "red" }
          Rectangle { id: b; objectName: "b"; color: "green" }
          Item { id: c; objectName: "c"; width: 50 }
        }
        SwipeView { id: empty; objectName: "empty"; y: 300 }
        function att(i) { var s = i.SwipeView; return [s.index, s.isCurrentItem, s.isNextItem, s.isPreviousItem].join(" ") }
        function attached() { seen = att(a) + " " + (a.SwipeView.view === view) + " / " + att(b) }
        function outside() { seen = root.SwipeView.index + " " + root.SwipeView.isCurrentItem + " " + root.SwipeView.view }
        function moved() { seen = [a.SwipeView.isCurrentItem, b.SwipeView.isCurrentItem, a.SwipeView.isPreviousItem, c.SwipeView.isNextItem].join(" ") }
        function takeFirst() { var t = view.takeItem(0); seen = t.SwipeView.index + " " + t.SwipeView.view + " " + t.width + " " + b.SwipeView.index }
      }
      """

  @api-QtQuick.Controls.SwipeView.interactive @api-QtQuick.Controls.SwipeView.orientation
  Scenario: A swipe view is horizontal and interactive
    Then the property "count" of "view" is 3
    And the property "currentIndex" of "view" is 0
    And the property "interactive" of "view" is true
    And the property "orientation" of "view" is 1
    And the property "horizontal" of "view" is true
    And the property "vertical" of "view" is false
    And the property "spacing" of "view" is 0
    And the property "focusPolicy" of "view" is 1

  Scenario: Every page is as large as the view, side by side
    Then the property "width" of "a" is 300
    And the property "height" of "a" is 200
    And the property "x" of "b" is 300
    And the property "x" of "c" is 600

  Scenario: A page that was given a size is still as large as the view
    Then the property "width" of "c" is 300
    And the property "height" of "c" is 200

  Scenario: An empty swipe view is empty
    Then the property "count" of "empty" is 0
    And the property "currentIndex" of "empty" is -1
    And the property "implicitWidth" of "empty" is 0

  @api-QtQuick.Controls.SwipeView.index @api-QtQuick.Controls.SwipeView.isCurrentItem @api-QtQuick.Controls.SwipeView.isNextItem @api-QtQuick.Controls.SwipeView.isPreviousItem @api-QtQuick.Controls.SwipeView.view
  Scenario: A page knows its place in the view
    When the method "attached" of "root" is called
    Then the property "seen" of "root" is "0 true false false true / 1 false true false"

  Scenario: An item outside a swipe view is in none
    When the method "outside" of "root" is called
    Then the property "seen" of "root" is "-1 false null"

  Scenario: Setting the current index moves the view to the page
    When the property "currentIndex" of "view" is set to 1
    And 1 second passes
    And the method "moved" of "root" is called
    Then the property "seen" of "root" is "false true true true"
    And the property "contentItem.contentX" of "view" is 300

  Scenario: The current index stays within the pages
    When the method "incrementCurrentIndex" of "view" is called
    And the method "incrementCurrentIndex" of "view" is called
    And the method "incrementCurrentIndex" of "view" is called
    Then the property "currentIndex" of "view" is 2
    When the method "decrementCurrentIndex" of "view" is called
    And the method "decrementCurrentIndex" of "view" is called
    And the method "decrementCurrentIndex" of "view" is called
    Then the property "currentIndex" of "view" is 0

  Scenario: A swipe to the left shows the next page
    When the user presses the pointer at 250, 100
    And the user moves the pointer to 200, 100
    And the user moves the pointer to 100, 100
    And the user moves the pointer to 50, 100
    And 500 milliseconds pass
    And the user releases the pointer at 50, 100
    And 1 second passes
    Then the property "currentIndex" of "view" is 1

  Scenario: A short swipe falls back to the page it left
    When the user presses the pointer at 250, 100
    And the user moves the pointer to 230, 100
    And the user moves the pointer to 210, 100
    And 500 milliseconds pass
    And the user releases the pointer at 210, 100
    And 1 second passes
    Then the property "currentIndex" of "view" is 0
    And the property "contentItem.contentX" of "view" is 0

  Scenario: A swipe view that is not interactive does not follow a swipe
    When the property "interactive" of "view" is set to false
    And the user presses the pointer at 250, 100
    And the user moves the pointer to 200, 100
    And the user moves the pointer to 100, 100
    And the user moves the pointer to 50, 100
    And 500 milliseconds pass
    And the user releases the pointer at 50, 100
    And 1 second passes
    Then the property "currentIndex" of "view" is 0

  Scenario: A vertical swipe view says so
    When the property "orientation" of "view" is set to 2
    Then the property "horizontal" of "view" is false
    And the property "vertical" of "view" is true
    And the property "width" of "a" is 300
    And the property "height" of "a" is 200

  Scenario: A page taken out of the view keeps its size and is in no view
    When the method "takeFirst" of "root" is called
    Then the property "seen" of "root" is "-1 null 300 0"
    And the property "currentIndex" of "view" is 0
    And the property "count" of "view" is 2
