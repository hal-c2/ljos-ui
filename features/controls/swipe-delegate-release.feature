# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-swipedelegate.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-swipedelegate.html#swipe-prop
# The Qt oracle drags in one go, too quickly for these. The values were
# measured on Qt 6.11.2 with qmltestrunner (offscreen) instead, by
# mousePress, mouseMove and mouseRelease with the same timings. The
# offscreen platform there reads Qt.styleHints.startDragDistance as 10; a
# desktop, and Ljós, read 8, so the swipe starts past 8 pixels here.

@ljos-only
Feature: Where a swiped row goes when it is let go
  A swipe starts once the pointer has gone further across the row than
  `Qt.styleHints.startDragDistance`, 8 pixels. Let go, the row opens to the side it was swiped to if it was swiped
  more than half the way, or if the pointer went that way at more than 300
  pixels a second on average since the press. Otherwise it closes.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        id: root; objectName: "root"; width: 400; height: 300; property string log: ""
        SwipeDelegate {
          id: d; objectName: "d"; width: 300; height: 50; text: "row"
          swipe.left: Rectangle { width: 100; height: 50 }
          swipe.right: Rectangle { width: 80; height: 50; anchors.right: parent.right }
          onClicked: root.log += "clicked "; onCanceled: root.log += "canceled "
          swipe.onOpened: root.log += "opened "; swipe.onClosed: root.log += "closed "
        }
        function openLeft() { d.swipe.open(SwipeDelegate.Left) }
        function clear() { log = "" }
      }
      """

  @api-QtQuick.Controls.SwipeDelegate.swipe
  Scenario: The swipe starts past the start drag distance and follows the pointer from the press
    When the user presses the pointer at 20, 25
    And 100 milliseconds pass
    And the user moves the pointer to 28, 25
    Then the property "swipe.position" of "d" is 0
    When 100 milliseconds pass
    And the user moves the pointer to 29, 25
    Then the property "swipe.position" of "d" is 0.09
    And the property "contentItem.x" of "d" is 21
    When 100 milliseconds pass
    And the user moves the pointer to 50, 30
    Then the property "swipe.position" of "d" is 0.3
    And the property "pressed" of "d" is true

  Scenario: Let go slowly short of half the way, it closes
    When the user presses the pointer at 20, 25
    And 100 milliseconds pass
    And the user moves the pointer to 40, 25
    And 100 milliseconds pass
    And the user moves the pointer to 61, 25
    And 300 milliseconds pass
    And the user releases the pointer at 61, 25
    And 1 second passes
    Then the property "swipe.position" of "d" is 0
    And the property "log" of "root" is "canceled closed "

  Scenario: Let go slowly at half the way, it closes
    When the user presses the pointer at 20, 25
    And 100 milliseconds pass
    And the user moves the pointer to 40, 25
    And 100 milliseconds pass
    And the user moves the pointer to 70, 25
    And 300 milliseconds pass
    And the user releases the pointer at 70, 25
    And 1 second passes
    Then the property "swipe.position" of "d" is 0
    And the property "log" of "root" is "canceled closed "

  Scenario: Let go slowly past half the way, it opens
    When the user presses the pointer at 20, 25
    And 100 milliseconds pass
    And the user moves the pointer to 40, 25
    And 100 milliseconds pass
    And the user moves the pointer to 81, 25
    And 300 milliseconds pass
    And the user releases the pointer at 81, 25
    And 1 second passes
    Then the property "swipe.position" of "d" is 1
    And the property "swipe.complete" of "d" is true
    And the property "log" of "root" is "canceled opened "

  Scenario: Flicked, it opens however little it was swiped
    When the user presses the pointer at 20, 25
    And 10 milliseconds pass
    And the user moves the pointer to 32, 25
    And 10 milliseconds pass
    And the user moves the pointer to 36, 25
    And 10 milliseconds pass
    And the user moves the pointer to 40, 25
    And the user releases the pointer at 40, 25
    And 1 second passes
    Then the property "swipe.position" of "d" is 1
    And the property "log" of "root" is "canceled opened "

  Scenario: A flick held before it is let go is slow on average, and closes
    When the user presses the pointer at 20, 25
    And 10 milliseconds pass
    And the user moves the pointer to 32, 25
    And 10 milliseconds pass
    And the user moves the pointer to 36, 25
    And 10 milliseconds pass
    And the user moves the pointer to 40, 25
    And 50 milliseconds pass
    And the user releases the pointer at 40, 25
    And 1 second passes
    Then the property "swipe.position" of "d" is 0
    And the property "log" of "root" is "canceled closed "

  Scenario: A flick after a long press is slow on average, and closes
    When the user presses the pointer at 20, 25
    And 500 milliseconds pass
    And the user moves the pointer to 21, 25
    And 10 milliseconds pass
    And the user moves the pointer to 32, 25
    And 10 milliseconds pass
    And the user moves the pointer to 36, 25
    And 10 milliseconds pass
    And the user moves the pointer to 40, 25
    And the user releases the pointer at 40, 25
    And 1 second passes
    Then the property "swipe.position" of "d" is 0

  Scenario: Flicked to the left, it opens to the right
    When the user presses the pointer at 250, 25
    And 10 milliseconds pass
    And the user moves the pointer to 238, 25
    And 10 milliseconds pass
    And the user moves the pointer to 233, 25
    And 10 milliseconds pass
    And the user moves the pointer to 228, 25
    And the user releases the pointer at 228, 25
    And 1 second passes
    Then the property "swipe.position" of "d" is -1
    And the property "log" of "root" is "canceled opened "

  Scenario: An open row flicked back a little stays open
    When the method "openLeft" of "root" is called
    And 1 second passes
    And the method "clear" of "root" is called
    And the user presses the pointer at 200, 25
    And 10 milliseconds pass
    And the user moves the pointer to 188, 25
    And 10 milliseconds pass
    And the user moves the pointer to 180, 25
    And the user releases the pointer at 180, 25
    And 1 second passes
    Then the property "swipe.position" of "d" is 1
    And the property "log" of "root" is "canceled opened "

  Scenario: An open row taken back past half the way closes
    When the method "openLeft" of "root" is called
    And 1 second passes
    And the method "clear" of "root" is called
    And the user presses the pointer at 200, 25
    And 100 milliseconds pass
    And the user moves the pointer to 160, 25
    And 100 milliseconds pass
    And the user moves the pointer to 130, 25
    And 300 milliseconds pass
    And the user releases the pointer at 130, 25
    And 1 second passes
    Then the property "swipe.position" of "d" is 0
    And the property "swipe.complete" of "d" is false
    And the property "log" of "root" is "canceled closed "
