# Sources:
#   docs/ARCHITECTURE.md section 19.4 (`app_frames`' `wakes` with `wake`
#   and `last_input`)
#   https://doc.qt.io/qt-6/qml-qtquick-mousearea.html
# The inspector is a Ljós extension: Qt has nothing to compare it with.

@ljos-only @headless
Feature: An agent reads what one input did
  Why did this click take 40 ms, and why did it redraw everything? While
  wakes are kept, `app_frames` with `wakes` and `wake` (a wake's number)
  or `last_input` lists what one wake did, in the order it did it: the
  handlers that ran, the properties that changed with their old and new
  values, the bindings that ran again, and the items the scene laid out,
  placed and redrew, each with the time it took. Only the latest wakes
  that were not an agent's own requests keep this.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        id: top
        objectName: "root"
        width: 200; height: 100
        property int clicks: 0
        Text { objectName: "label"; text: "clicked " + top.clicks }
        MouseArea { objectName: "area"; anchors.fill: parent; onClicked: top.clicks += 1 }
      }
      """
    And the inspector is connected with the capabilities "inspect, input"

  Scenario: A click lists its handler, the change it made and what that ran
    When the agent calls the tool "app_frames" with:
      """json
      {"wakes": true, "record": true}
      """
    And the agent calls the tool "app_click" with:
      """json
      {"target": "#area"}
      """
    And the agent calls the tool "app_frames" with:
      """json
      {"wakes": true, "last_input": true}
      """
    Then the answer's "/causes/0" is "input"
    And the answer's "/counts/handler" is at least 1
    And the answer's "/counts/binding" is at least 1
    And the answer's "/counts/redrawn" is at least 1
    And the answer's text contains "handler MouseArea.onClicked at file:///Scenario.qml:8:"
    And the answer's text contains "changed Scenario.clicks"
    And the answer's text contains "0 → 1"
    And the answer's text contains "binding Text.text at file:///Scenario.qml:7:"

  Scenario: A wake that is not kept is refused
    When the agent calls the tool "app_frames" with:
      """json
      {"wakes": true, "record": true}
      """
    And the agent calls the tool "app_click" with:
      """json
      {"target": "#area"}
      """
    And the agent calls the tool "app_frames" with:
      """json
      {"wakes": true, "last_input": true}
      """
    And the agent calls the tool "app_frames" with:
      """json
      {"wakes": true, "wake": 999}
      """
    Then the agent is refused with "invalid_argument"
    And the refusal mentions "no wake 999"

  Scenario: Nothing is kept for a wake while no client keeps wakes
    When the agent calls the tool "app_click" with:
      """json
      {"target": "#area"}
      """
    And the agent calls the tool "app_frames" with:
      """json
      {"wakes": true, "last_input": true}
      """
    Then the agent is refused with "invalid_argument"
    And the refusal mentions "record: true"

  Scenario: A wake's number goes with the wakes
    When the agent calls the tool "app_frames" with:
      """json
      {"last_input": true}
      """
    Then the agent is refused with "invalid_argument"
    And the refusal mentions "go with `wakes`"
