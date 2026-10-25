# Sources:
#   docs/ARCHITECTURE.md sections 19.4 (`app_watch`) and principle 6
# The inspector is a Ljós extension: Qt has nothing to compare it with.

@ljos-only @desktop
Feature: An agent waits until the application has seen the input it sent
  Input sent from outside, through the compositor, reaches the
  application on a socket of its own, so the client that sent it cannot
  tell from the sending when the application has seen it. Each time the
  application's UI thread goes to sleep and what it says is new,
  `app_watch` counts a rest (`rests`, asked for by name): when the thread
  wakes by itself next (`wake_in_ms`, `null` when only input or a message
  wakes it) and the pointer as the primary window's last pass saw it, in
  the window's points. A watch waits on the server's thread, so the
  application runs no pass for it.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        width: 200; height: 100; color: "white"
        MouseArea {
          objectName: "area"
          property bool held: pressed
          anchors.fill: parent
        }
      }
      """

  Scenario: The pointer is told as the last pass before the rest saw it
    When the user presses the pointer at 50, 30
    And the agent calls the tool "app_watch" with:
      """json
      {"topics": ["rests"]}
      """
    Then the answer's "/rest/pointer/x" is 50
    And the answer's "/rest/pointer/y" is 30
    And the answer's "/rest/pointer/pressed" is true
    And the property "held" of "area" is true
    When the user releases the pointer at 120, 60
    And the agent calls the tool "app_watch" with:
      """json
      {"topics": ["rests"]}
      """
    Then the answer's "/rest/pointer/x" is 120
    And the answer's "/rest/pointer/y" is 60
    And the answer's "/rest/pointer/pressed" is false
    And the property "held" of "area" is false

  Scenario: The rests are counted from the first, and told in words too
    When the agent calls the tool "app_watch"
    Then the answer's "/counts/rests" is at least 1
    And the answer's text contains "The UI thread"
