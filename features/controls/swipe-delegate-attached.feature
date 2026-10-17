# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-swipedelegate.html#attached-properties
#   https://doc.qt.io/qt-6/qml-qtquick-controls-swipedelegate.html#clicked-signal
# Verified on Qt 6.11.2 (qmltestrunner and qml6 under a real pointer,
# offscreen and on X11): the order of the log is Qt's, the delegate saying
# it is pressed once more after `canceled` included.

@qt-parity
Feature: The side items of a swipe delegate are pressed and clicked
  The item a row is swiped open to show takes the presses on it: its
  attached `SwipeDelegate.pressed` is true while it is held, and let go,
  it says `SwipeDelegate.clicked`. The row itself is still not clicked,
  and says it is open again. Only an open row hands the press on; a swipe
  that starts on the item lets go of it.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        id: root; objectName: "root"; width: 400; height: 300; property string log: ""
        SwipeDelegate {
          id: d; objectName: "d"; width: 300; height: 50; text: "row"
          swipe.left: Rectangle {
            objectName: "l"; width: 100; height: 50; property bool held: SwipeDelegate.pressed
            onHeldChanged: root.log += "left" + held + " "; SwipeDelegate.onClicked: root.log += "leftclicked "
          }
          swipe.right: Rectangle {
            width: 80; height: 50; anchors.right: parent.right; property bool held: SwipeDelegate.pressed
            onHeldChanged: root.log += "right" + held + " "; SwipeDelegate.onClicked: root.log += "rightclicked "
          }
          onClicked: root.log += "clicked "; onCanceled: root.log += "canceled "
          onPressedChanged: root.log += "pressed" + pressed + " "
          swipe.onOpened: root.log += "opened "; swipe.onClosed: root.log += "closed "
        }
        function openLeft() { d.swipe.open(SwipeDelegate.Left) } function openRight() { d.swipe.open(SwipeDelegate.Right) }
        function clear() { log = "" }
      }
      """

  @api-QtQuick.Controls.SwipeDelegate.pressed @api-QtQuick.Controls.SwipeDelegate.clicked
  Scenario: The item shown is pressed while it is held, and clicked when it is let go
    When the method "openLeft" of "root" is called
    And 1 second passes
    And the method "clear" of "root" is called
    And the user presses the pointer at 30, 25
    Then the property "held" of "l" is true
    And the property "log" of "root" is "pressedtrue lefttrue "
    When the user releases the pointer at 30, 25
    Then the property "held" of "l" is false
    And the property "swipe.position" of "d" is 1
    And the property "log" of "root" is "pressedtrue lefttrue pressedfalse opened leftfalse leftclicked canceled pressedtrue opened pressedfalse "

  Scenario: The item on the right is clicked as the one on the left
    When the method "openRight" of "root" is called
    And 1 second passes
    And the method "clear" of "root" is called
    And the user presses the pointer at 260, 25
    And the user releases the pointer at 260, 25
    Then the property "log" of "root" is "pressedtrue righttrue pressedfalse opened rightfalse rightclicked canceled pressedtrue opened pressedfalse "

  Scenario: A press beside the item is the row's
    When the method "openRight" of "root" is called
    And 1 second passes
    And the method "clear" of "root" is called
    And the user presses the pointer at 100, 25
    And the user releases the pointer at 100, 25
    Then the property "log" of "root" is "pressedtrue pressedfalse opened canceled "

  Scenario: A row that is not open keeps the press
    When the property "swipe.position" of "d" is set to 0.5
    And the user presses the pointer at 30, 25
    And the user releases the pointer at 30, 25
    Then the property "log" of "root" is "pressedtrue pressedfalse canceled "
    And the property "swipe.position" of "d" is 0.5

  Scenario: A swipe that starts on the item lets go of it
    When the method "openLeft" of "root" is called
    And 1 second passes
    And the method "clear" of "root" is called
    And the user drags from 90, 25 to 10, 25
    And 1 second passes
    Then the property "swipe.position" of "d" is 0
    And the property "held" of "l" is false
    And the property "log" of "root" is "pressedtrue lefttrue leftfalse pressedfalse canceled closed "
