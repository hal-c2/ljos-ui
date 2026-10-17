# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-swipedelegate.html#attached-properties
#   https://doc.qt.io/qt-6/qml-qtquick-controls-swipedelegate.html#swipe-prop
# Verified on Qt 6.11.2 (qmltestrunner and the oracle, offscreen).

@qt-parity
Feature: A press on a side item that does not end on it
  A side item of an open row is clicked only when the press on it is let
  go on it. Let go anywhere else, it is not clicked; it is let go of
  last, after the row has said it is open again. A press on it that
  swipes lets go of it at once; when the row stays open, the row is
  pressed once more and says it is open, or closed while it springs
  back, as Qt's does. The item
  behind a row is no side item: it is never pressed or clicked.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        id: root; objectName: "root"; width: 400; height: 300; property string log: ""; property string side: ""
        SwipeDelegate {
          id: d; objectName: "d"; width: 300; height: 50; text: "row"
          swipe.left: Rectangle {
            width: 100; height: 50; property bool held: SwipeDelegate.pressed
            onHeldChanged: { root.log += "left" + held + " "; root.side += "left" + held + " " }
            SwipeDelegate.onClicked: { root.log += "leftclicked "; root.side += "leftclicked " }
          }
          onClicked: root.log += "clicked "; onCanceled: root.log += "canceled "; onPressedChanged: root.log += "pressed" + pressed + " "
          swipe.onOpened: root.log += "opened "; swipe.onClosed: root.log += "closed "
        }
        SwipeDelegate {
          id: b; objectName: "b"; y: 100; width: 300; height: 50; text: "behind"
          swipe.behind: Rectangle {
            width: 300; height: 50; property bool held: SwipeDelegate.pressed
            onHeldChanged: root.log += "behind" + held + " "; SwipeDelegate.onClicked: root.log += "behindclicked "
          }
          onClicked: root.log += "clicked "; onCanceled: root.log += "canceled "; swipe.onOpened: root.log += "opened "
        }
        function openLeft() { d.swipe.open(SwipeDelegate.Left); b.swipe.open(SwipeDelegate.Left) }
        function clear() { log = ""; side = "" }
      }
      """
    When the method "openLeft" of "root" is called
    And 1 second passes
    And the method "clear" of "root" is called

  @api-QtQuick.Controls.SwipeDelegate.clicked
  Scenario: Off the row the row is let up, the side item held, and let go there it is not clicked
    When the user presses the pointer at 30, 25
    And the user moves the pointer to 30, 80
    Then the property "pressed" of "d" is false
    And the property "swipe.leftItem.held" of "d" is true
    When the user moves the pointer to 30, 25
    Then the property "pressed" of "d" is true
    When the user moves the pointer to 30, 280
    And the user releases the pointer at 30, 280
    And 1 second passes
    Then the property "side" of "root" is "lefttrue leftfalse "
    And the property "pressed" of "d" is false
    And the property "swipe.position" of "d" is 1

  Scenario: A swipe outward from the side item lets go of it and leaves the row open
    When the user presses the pointer at 50, 25
    And the user moves the pointer to 62, 25
    Then the property "log" of "root" is "pressedtrue lefttrue leftfalse "
    When the user moves the pointer to 90, 25
    And the user releases the pointer at 90, 25
    And 1 second passes
    Then the property "log" of "root" is "pressedtrue lefttrue leftfalse pressedfalse canceled opened pressedtrue opened pressedfalse "
    And the property "swipe.position" of "d" is 1

  @api-QtQuick.Controls.SwipeDelegate.swipe
  Scenario: A short swipe inward from the side item springs back and says so as Qt does
    When the user presses the pointer at 50, 25
    And the user moves the pointer to 38, 25
    Then the property "swipe.complete" of "d" is false
    And the property "swipe.position" of "d" is 0.88
    When the user moves the pointer to 30, 25
    And the user releases the pointer at 30, 25
    And 1 second passes
    Then the property "log" of "root" is "pressedtrue lefttrue leftfalse pressedfalse canceled pressedtrue closed pressedfalse opened "
    And the property "swipe.position" of "d" is 1
    And the property "swipe.complete" of "d" is true

  @api-QtQuick.Controls.SwipeDelegate.pressed
  Scenario: The item behind a row is not pressed or clicked
    When the user presses the pointer at 30, 125
    And the user releases the pointer at 30, 125
    And 1 second passes
    Then the property "log" of "root" is "opened canceled "
    And the property "swipe.position" of "b" is 1
