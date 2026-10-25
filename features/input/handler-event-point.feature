# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-taphandler.html#tapped-signal
#   https://doc.qt.io/qt-6/qml-qtquick-pointerhandler.html#grabChanged-signal
#   https://doc.qt.io/qt-6/qml-qtquick-eventpoint.html
# Verified on Qt 6.11.2 (qml6, offscreen). The right button and the rest
# of the eventPoint are in crates/ljos_test/tests/qml/suite/tst_handler_event_point.qml.

@qt-parity
Feature: The eventPoint a pointer handler's signals are given
  A TapHandler's tapped, singleTapped and canceled, and every pointer
  handler's grabChanged, are given the eventPoint: where the point is and
  where it went down, in the handler's item and in the scene, and its state.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        width: 400; height: 400
        property string log: ""
        property string single: ""
        function at(p) { return p.x + "," + p.y }
        function show(e) {
          return at(e.position) + " in " + at(e.scenePosition) + " from " + at(e.pressPosition)
            + " in " + at(e.scenePressPosition) + " state " + e.state
        }
        Item {
          x: 50; y: 30; width: 200; height: 200
          TapHandler {
            onTapped: (eventPoint, button) => root.log += "tapped " + root.show(eventPoint) + "|"
            onSingleTapped: eventPoint => root.single = root.at(eventPoint.position)
            onCanceled: point => root.log += "canceled " + root.show(point) + "|"
          }
        }
        Item {
          x: 50; y: 250; width: 100; height: 100
          DragHandler { onGrabChanged: (transition, point) => root.log += transition + " " + root.show(point) + "|" }
        }
      }
      """

  Scenario: A tap is given where the button came up
    When the user presses the pointer at 70, 40
    And the user releases the pointer at 70, 40
    Then the property "log" of "root" is "tapped 20,10 in 70,40 from 20,10 in 70,40 state 8|"

  Scenario: A single tap is given the same point
    When the user presses the pointer at 80, 70
    And the user releases the pointer at 80, 70
    Then the property "single" of "root" is "30,40"

  Scenario: A cancel is given where the pointer went
    When the user presses the pointer at 70, 40
    And the user moves the pointer to 170, 150
    And the user releases the pointer at 170, 150
    Then the property "log" of "root" is "canceled 120,120 in 170,150 from 20,10 in 70,40 state 2|"

  Scenario: A passive grab is given the point that went down
    When the user presses the pointer at 60, 260
    Then the property "log" of "root" is "1 10,10 in 60,260 from 10,10 in 60,260 state 1|"
