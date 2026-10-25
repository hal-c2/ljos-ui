# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-taphandler.html#gesturePolicy-prop
#   https://doc.qt.io/qt-6/qml-qtquick-pointerhandler.html#grabChanged-signal
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: The grabs of a TapHandler
  A TapHandler says when it takes the point and when it lets it go, as
  every pointer handler does. With the DragThreshold policy it only watches
  the point, a passive grab, and lets go of it when the button comes up,
  whatever happened in between. With the other policies it holds the point
  on its own until the tap is over. Where in the item a point that is let
  go of is, qml6 tells one way or another depending on how the event came:
  only its place in the scene is checked.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        width: 400; height: 400
        property string log: ""
        property int policy: TapHandler.DragThreshold
        function at(p) { return p.x + "," + p.y }
        function grab(t, p) {
          return t + " " + p.state + " " + (t === 1 || t === 16 ? at(p.position) + " in " : "") + at(p.scenePosition)
        }
        Item {
          x: 50; y: 30; width: 200; height: 200
          TapHandler {
            gesturePolicy: root.policy
            acceptedButtons: Qt.LeftButton | Qt.RightButton
            onGrabChanged: (transition, point) => root.log += root.grab(transition, point) + "|"
            onPressedChanged: root.log += (pressed ? "pressed|" : "released|")
            onTapped: root.log += "tapped|"
            onCanceled: root.log += "canceled|"
          }
        }
      }
      """

  Rule: A handler that only watches the point

    Scenario: A tap is watched from the press to the release
      When the user presses the pointer at 70, 40
      And the user releases the pointer at 70, 40
      Then the property "log" of "root" is "1 1 20,10 in 70,40|pressed|tapped|released|2 8 70,40|"

    Scenario: A drag cancels the tap but the point is watched until it comes up
      When the user presses the pointer at 70, 40
      And the user moves the pointer to 170, 150
      Then the property "log" of "root" is "1 1 20,10 in 70,40|pressed|released|canceled|"
      When the property "log" of "root" is set to ""
      And the user releases the pointer at 170, 150
      Then the property "log" of "root" is "2 8 170,150|"

    Scenario: A tap of the right button is watched as one of the left
      When the user clicks the right button at 70, 40
      Then the property "log" of "root" is "1 1 20,10 in 70,40|pressed|tapped|released|2 8 70,40|"

  Rule: A handler that holds the point on its own

    Scenario: A tap is held from the press to the release
      When the property "policy" of "root" is set to 2
      And the user presses the pointer at 70, 40
      And the user releases the pointer at 70, 40
      Then the property "log" of "root" is "16 1 20,10 in 70,40|pressed|tapped|released|32 8 70,40|"

    Scenario: Leaving the bounds lets go before the tap is cancelled
      When the property "policy" of "root" is set to 1
      And the user presses the pointer at 70, 40
      And the user moves the pointer to 350, 330
      Then the property "log" of "root" is "16 1 20,10 in 70,40|pressed|released|32 2 350,330|canceled|"

    Scenario: A release outside lets go before the tap is cancelled
      When the property "policy" of "root" is set to 2
      And the user presses the pointer at 70, 40
      And the user moves the pointer to 350, 330
      And the user releases the pointer at 350, 330
      Then the property "log" of "root" is "16 1 20,10 in 70,40|pressed|released|32 8 350,330|canceled|"
