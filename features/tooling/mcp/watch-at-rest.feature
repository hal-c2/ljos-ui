# Sources:
#   docs/ARCHITECTURE.md sections 19.4 (`app_watch`) and principle 6
# The inspector is a Ljós extension: Qt has nothing to compare it with.

@ljos-only @desktop
Feature: An agent waits until the application is at rest after its input
  Input an agent sends moves the application on for a while: a pass runs,
  an animation starts and ends. `app_watch` with `until_rest` answers when
  the picture has stopped moving, or after `wait_ms` if it never does, and
  `rest.at_rest` says which. A repeating timer does not stop a rest: nothing
  on the screen moves for it, and `wake_in_ms` tells when it comes.

  Scenario: The answer after a click waits for the animation the click started
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Item {
        width: 300; height: 200
        Rectangle {
          id: box
          objectName: "box"
          y: 100; width: 20; height: 20
          Behavior on x { NumberAnimation { duration: 400 } }
        }
        Button { id: go; text: "Go"; onClicked: box.x = 100 }
      }
      """
    When the agent calls the tool "app_click" with:
      """json
      {"target": "#go"}
      """
    And the agent calls the tool "app_watch" with:
      """json
      {"until_rest": true, "wait_ms": 10000}
      """
    Then the answer's "/rest/at_rest" is true
    And the answer's "/rest/animating" is false
    And the answer's "/waited" is true
    And the answer's text contains "at rest"
    And the property "x" of "box" is 100

  Scenario: An animation that never ends is told as no rest when the wait is over
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        width: 200; height: 100
        Rectangle {
          width: 10; height: 10
          NumberAnimation on x { from: 0; to: 100; duration: 500; loops: Animation.Infinite }
        }
      }
      """
    When the agent calls the tool "app_watch" with:
      """json
      {"until_rest": true, "wait_ms": 300}
      """
    Then the answer's "/rest/at_rest" is false
    And the answer's "/rest/animating" is true
    And the answer's text contains "not at rest"

  Scenario: A paused animation moves nothing, so the application is at rest
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        id: top
        width: 200; height: 100
        NumberAnimation { target: top; property: "x"; to: 100; duration: 500; paused: true; running: true }
      }
      """
    When the agent calls the tool "app_watch" with:
      """json
      {"until_rest": true, "wait_ms": 10000}
      """
    Then the answer's "/rest/at_rest" is true

  Scenario: A repeating timer does not stop the rest, and its wake is told
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        width: 200; height: 100
        Timer { interval: 60000; repeat: true; running: true }
      }
      """
    When the agent calls the tool "app_watch" with:
      """json
      {"until_rest": true, "wait_ms": 10000}
      """
    Then the answer's "/rest/at_rest" is true
    And the answer's "/rest/wake_in_ms" is at least 1
