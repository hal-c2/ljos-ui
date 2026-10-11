# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-transition.html
#   https://doc.qt.io/qt-6/qtquick-statesanimations-animations.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A change of state while a transition runs
  A change of state ends the transition that is running. The next one
  starts from where the properties are.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 400; height: 400
        property alias toRightRuns: toRight.running
        property alias toFarRuns: toFar.running
        Rectangle { id: box; objectName: "box"; width: 10; height: 10 }
        states: [
          State { name: "right"; PropertyChanges { target: box; x: 200 } },
          State { name: "far"; PropertyChanges { target: box; x: 1000 } },
          State { name: "jump"; PropertyChanges { target: box; x: 300 } }
        ]
        transitions: [
          Transition { id: toRight; to: "right"
            NumberAnimation { property: "x"; duration: 2000 } },
          Transition { id: toFar; to: "far"
            NumberAnimation { property: "x"; duration: 2000 } },
          Transition { to: ""
            NumberAnimation { property: "x"; duration: 2000 } }
        ]
      }
      """

  Scenario: A change no transition is for gives the property its value at once
    When the property "state" of "root" is set to "right"
    And 1 second passes
    And the property "state" of "root" is set to "jump"
    Then the property "x" of "box" is 300

  Scenario: The transition that was running is over
    When the property "state" of "root" is set to "right"
    And 1 second passes
    And the property "state" of "root" is set to "jump"
    Then the property "toRightRuns" of "root" is false

  Scenario: A property that was given its value at once stays there
    When the property "state" of "root" is set to "right"
    And 1 second passes
    And the property "state" of "root" is set to "jump"
    And 3 seconds pass
    Then the property "x" of "box" is 300

  Scenario: The next transition starts from where the property is
    When the property "state" of "root" is set to "right"
    And 1 second passes
    And the property "state" of "root" is set to "far"
    Then the property "x" of "box" is at least 50
    And the property "x" of "box" is at most 150

  Scenario: The next transition is the one that runs
    When the property "state" of "root" is set to "right"
    And 1 second passes
    And the property "state" of "root" is set to "far"
    Then the property "toRightRuns" of "root" is false
    And the property "toFarRuns" of "root" is true

  Scenario: The next transition takes the property to the new state's value
    When the property "state" of "root" is set to "right"
    And 1 second passes
    And the property "state" of "root" is set to "far"
    And 3 seconds pass
    Then the property "x" of "box" is 1000

  Scenario: Going back while on the way starts from where the property is
    When the property "state" of "root" is set to "right"
    And 1 second passes
    And the property "state" of "root" is set to ""
    Then the property "x" of "box" is at least 50
    And the property "x" of "box" is at most 150

  Scenario: Going back while on the way ends at the base state's value
    When the property "state" of "root" is set to "right"
    And 1 second passes
    And the property "state" of "root" is set to ""
    And 3 seconds pass
    Then the property "x" of "box" is 0

  Scenario: The base state's value comes back after a transition that was cut short
    When the property "state" of "root" is set to "right"
    And 1 second passes
    And the property "state" of "root" is set to "jump"
    And the property "state" of "root" is set to ""
    And 3 seconds pass
    Then the property "x" of "box" is 0
