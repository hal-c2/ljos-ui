# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-behavior.html
#   https://doc.qt.io/qt-6/qtquick-statesanimations-behaviors.html
#   https://doc.qt.io/qt-6/qml-qtquick-transition.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A Behavior on a property that a state changes
  A Behavior animates the value a state gives its property, and the value
  that comes back. A transition that animates the property goes before
  the Behavior.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 400; height: 400
        property bool smooth: true
        Rectangle {
          id: box; objectName: "box"; width: 10; height: 10
          Behavior on y { enabled: root.smooth; NumberAnimation { duration: 2000 } }
          Behavior on x { NumberAnimation { duration: 8000 } }
        }
        states: [
          State { name: "down"; PropertyChanges { target: box; y: 200 } },
          State { name: "both"; PropertyChanges { target: box; x: 200; y: 200 } }
        ]
        transitions: Transition {
          to: "both"
          NumberAnimation { property: "x"; duration: 2000 }
        }
      }
      """

  Scenario: The value a state gives is not taken at once
    When the property "state" of "root" is set to "down"
    Then the property "y" of "box" is 0

  Scenario: The property is on its way to the state's value
    When the property "state" of "root" is set to "down"
    And 1 second passes
    Then the property "y" of "box" is at least 50
    And the property "y" of "box" is at most 150

  Scenario: The property arrives at the state's value
    When the property "state" of "root" is set to "down"
    And 3 seconds pass
    Then the property "y" of "box" is 200

  Scenario: The value that comes back is animated as well
    When the property "state" of "root" is set to "down"
    And 3 seconds pass
    And the property "state" of "root" is set to ""
    And 1 second passes
    Then the property "y" of "box" is at least 50
    And the property "y" of "box" is at most 150

  Scenario: A Behavior that is not enabled lets the state's value through
    When the property "smooth" of "root" is set to false
    And the property "state" of "root" is set to "down"
    Then the property "y" of "box" is 200

  Scenario: A transition that animates the property goes before its Behavior
    When the property "state" of "root" is set to "both"
    And 3 seconds pass
    Then the property "x" of "box" is 200

  Scenario: A property the transition does not animate is animated by its Behavior
    When the property "state" of "root" is set to "both"
    And 1 second passes
    Then the property "y" of "box" is at least 50
    And the property "y" of "box" is at most 150
