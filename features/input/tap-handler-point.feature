# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-taphandler.html#point-prop
#   https://doc.qt.io/qt-6/qml-qtquick-handlerpoint.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: The point of a TapHandler
  While it is pressed a TapHandler tells where the pointer is and where
  it went down, in its item and in the scene, and which buttons are held.
  When the press is over the point is empty again.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        width: 200; height: 200
        property string log: ""
        property string now: show(handler.point)
        function at(p) { return p.x + "," + p.y }
        function show(p) {
          return at(p.position) + " in " + at(p.scenePosition) + " from " + at(p.pressPosition)
            + " in " + at(p.scenePressPosition) + " b" + p.pressedButtons + " id " + p.id
        }
        Item {
          x: 50; y: 40; width: 100; height: 100
          TapHandler {
            id: handler
            onPointChanged: root.log += "point " + root.at(point.position) + "|"
            onPressedChanged: root.log += "pressed " + pressed + " " + root.show(point) + "|"
            onTapped: root.log += "tapped " + root.show(point) + "|"
            onCanceled: root.log += "canceled " + root.show(handler.point) + "|"
          }
        }
      }
      """

  Rule: While the handler is pressed

    Scenario: A handler that is not pressed has an empty point
      Then the property "now" of "root" is "0,0 in 0,0 from 0,0 in 0,0 b0 id -1"

    Scenario: A press fills the point in
      When the user presses the pointer at 60, 70
      Then the property "now" of "root" is "10,30 in 60,70 from 10,30 in 60,70 b1 id 0"

    Scenario: The point follows the pointer and remembers where it went down
      When the user presses the pointer at 60, 70
      And the user moves the pointer to 62, 73
      Then the property "now" of "root" is "12,33 in 62,73 from 10,30 in 60,70 b1 id 0"

    Scenario: The point is filled in before the handler says it is pressed
      When the user presses the pointer at 60, 70
      Then the property "log" of "root" is "pressed true 10,30 in 60,70 from 10,30 in 60,70 b1 id 0|point 10,30|"

    Scenario: A move without a press leaves the point empty
      When the user moves the pointer to 60, 70
      Then the property "now" of "root" is "0,0 in 0,0 from 0,0 in 0,0 b0 id -1"
      And the property "log" of "root" is ""

  Rule: When the press is over

    Scenario: The point is empty again after the release
      When the user presses the pointer at 60, 70
      And the user releases the pointer at 60, 70
      Then the property "now" of "root" is "0,0 in 0,0 from 0,0 in 0,0 b0 id -1"

    Scenario: A tap still reads where it was, with no button held
      When the user presses the pointer at 60, 70
      And the user moves the pointer to 62, 73
      And the property "log" of "root" is set to ""
      And the user releases the pointer at 62, 73
      Then the property "log" of "root" is "tapped 12,33 in 62,73 from 10,30 in 60,70 b0 id 0|pressed false 12,33 in 62,73 from 10,30 in 60,70 b0 id 0|point 0,0|"

    Scenario: A press that turns into a drag is cancelled with the point it had
      When the user presses the pointer at 60, 70
      And the property "log" of "root" is set to ""
      And the user moves the pointer to 120, 70
      Then the property "log" of "root" is "pressed false 10,30 in 60,70 from 10,30 in 60,70 b1 id 0|canceled 10,30 in 60,70 from 10,30 in 60,70 b1 id 0|point 0,0|"
