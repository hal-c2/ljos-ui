# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-swipedelegate.html#swipe-prop
# Measured on Qt 6.11.2 with qmltestrunner (offscreen), by mousePress,
# mouseMove and mouseRelease, every move past the start drag distance.

@qt-parity
Feature: A swipe that goes past the middle in one drag
  The side a drag shows is the side the row would be at if it followed
  the pointer: the position at the press, as a distance, plus the way
  the pointer went. The position is then the position at the press plus
  the way the pointer went over the width of the item on that side. Where
  the items are as wide, the row follows the pointer across the middle;
  where they are not, the position jumps as the side changes. A row
  open to one side is not opened to the other by `open()`.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        id: root; objectName: "root"; width: 400; height: 300
        SwipeDelegate {
          id: d; objectName: "d"; width: 300; height: 50; text: "row"
          swipe.left: Rectangle { width: 100; height: 50 }
          swipe.right: Rectangle { width: 80; height: 50; anchors.right: parent.right }
        }
        function openLeft() { d.swipe.open(SwipeDelegate.Left) } function openRight() { d.swipe.open(SwipeDelegate.Right) }
        function closeRow() { d.swipe.close() }
      }
      """

  @api-QtQuick.Controls.SwipeDelegate.swipe
  Scenario: From the middle, the row follows the pointer to one side and on to the other
    When the user presses the pointer at 150, 25
    And the user moves the pointer to 180, 25
    Then the property "swipe.position" of "d" is 0.3
    And the property "swipe.leftItem.visible" of "d" is true
    When the user moves the pointer to 130, 25
    Then the property "swipe.position" of "d" is -0.25
    And the property "swipe.leftItem.visible" of "d" is false
    And the property "swipe.rightItem.visible" of "d" is true
    And the property "contentItem.x" of "d" is -8
    When the user releases the pointer at 130, 25

  Scenario: Opened to the wider item, the row jumps as it crosses the middle
    When the method "openLeft" of "root" is called
    And 1 second passes
    And the user presses the pointer at 250, 25
    And the user moves the pointer to 200, 25
    Then the property "swipe.position" of "d" is 0.5
    When the user moves the pointer to 151, 25
    Then the property "swipe.position" of "d" is 0.01
    When the user moves the pointer to 149, 25
    Then the property "swipe.position" of "d" is -0.2625
    And the property "swipe.rightItem.visible" of "d" is true
    When the user moves the pointer to 180, 25
    Then the property "swipe.position" of "d" is 0.3
    When the user releases the pointer at 180, 25

  Scenario: Opened to the narrower item, the row crosses into the middle from the far side
    When the method "openRight" of "root" is called
    And 1 second passes
    And the user presses the pointer at 100, 25
    And the user moves the pointer to 170, 25
    Then the property "swipe.position" of "d" is -0.125
    When the user moves the pointer to 190, 25
    Then the property "swipe.position" of "d" is -0.1
    When the user moves the pointer to 200, 25
    Then the property "swipe.position" of "d" is 0
    When the user moves the pointer to 260, 25
    Then the property "swipe.position" of "d" is 0.6
    And the property "swipe.leftItem.visible" of "d" is true
    When the user releases the pointer at 260, 25

  @api-QtQuick.Controls.SwipeDelegate.swipe
  Scenario: open() does not take an open row to the other side
    When the method "openLeft" of "root" is called
    And 1 second passes
    And the method "openRight" of "root" is called
    And 1 second passes
    Then the property "swipe.position" of "d" is 1
    When the method "closeRow" of "root" is called
    And 1 second passes
    And the method "openRight" of "root" is called
    And 1 second passes
    And the method "openLeft" of "root" is called
    And 1 second passes
    Then the property "swipe.position" of "d" is -1
