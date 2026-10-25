# Sources:
#   docs/ARCHITECTURE.md section 19.4 (`app_frames`' `growth`)
#   https://doc.qt.io/qt-6/qml-qtquick-repeater.html
# The inspector is a Ljós extension: Qt has nothing to compare it with.

@ljos-only @headless
Feature: An agent finds the components whose instances keep growing
  A leak shows as a component whose instances go up and do not come back
  down, at each repeat of an action or while the application rests.
  `app_frames` with `growth` samples the instances and bytes of every
  component loaded while a client records: at a pass of the application
  at most once a second, and at each answer, so an application at rest is
  not woken for it. A run of equal samples is kept as its first and last.
  A component that went up at least twice and never down is marked
  growing, and the growing ones are listed first, each with its series.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property int n: 1
        Repeater { model: parent.n; Item {} }
      }
      """
    And the inspector is connected with the capabilities "inspect, mutate"

  Scenario: Nothing is sampled until a client asks
    When the agent calls the tool "app_frames" with:
      """json
      {"growth": true}
      """
    Then the answer's "/recording" is false
    And the answer's "/samples" is 0

  Scenario: A component made more of at each step is marked growing
    When the agent calls the tool "app_frames" with:
      """json
      {"growth": true, "record": true}
      """
    And the agent calls the tool "app_set_property" with:
      """json
      {"target": "#root", "property": "n", "value": 2}
      """
    And the agent calls the tool "app_frames" with:
      """json
      {"growth": true}
      """
    And the agent calls the tool "app_set_property" with:
      """json
      {"target": "#root", "property": "n", "value": 3}
      """
    And the agent calls the tool "app_frames" with:
      """json
      {"growth": true}
      """
    Then the answer's "/growing" is 1
    And the answer's "/components/0/growing" is true
    And the answer's "/components/0/first" is 1
    And the answer's "/components/0/last" is 3
    And the answer's "/components/0/name" is "Item"
    And the answer's text contains "1 of 2 keep growing"

  Scenario: One that comes back down is not
    When the agent calls the tool "app_frames" with:
      """json
      {"growth": true, "record": true}
      """
    And the agent calls the tool "app_set_property" with:
      """json
      {"target": "#root", "property": "n", "value": 3}
      """
    And the agent calls the tool "app_frames" with:
      """json
      {"growth": true}
      """
    And the agent calls the tool "app_set_property" with:
      """json
      {"target": "#root", "property": "n", "value": 1}
      """
    And the agent calls the tool "app_frames" with:
      """json
      {"growth": true}
      """
    Then the answer's "/growing" is 0
