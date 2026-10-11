# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-stategroup.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: StateGroup
  A StateGroup gives states to something that is not an item, or a second
  set of states to an item.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 200; height: 200
        property bool on: false
        property string log: ""
        Rectangle { id: box; objectName: "box"; width: 10; height: 10 }
        QtObject { id: data; objectName: "data"; property int n: 1 }
        StateGroup {
          id: sizes; objectName: "sizes"
          onStateChanged: root.log += "[" + state + "]"
          states: [
            State { name: "wide"; PropertyChanges { target: box; width: 100 } },
            State { name: "many"; PropertyChanges { target: data; n: 5 } }
          ]
        }
        StateGroup {
          objectName: "shapes"
          states: State { name: "tall"; when: root.on
            PropertyChanges { target: box; height: 100 } }
        }
      }
      """

  @api-QtQuick.StateGroup.state
  Scenario: A group starts in the base state
    Then the property "state" of "sizes" is ""

  @api-QtQuick.StateGroup.states
  Scenario: Entering a state of a group makes its changes
    When the property "state" of "sizes" is set to "wide"
    Then the property "width" of "box" is 100

  Scenario: A state of a group changes an object that is not an item
    When the property "state" of "sizes" is set to "many"
    Then the property "n" of "data" is 5

  Scenario: Leaving a state of a group puts its changes back
    When the property "state" of "sizes" is set to "many"
    And the property "state" of "sizes" is set to ""
    Then the property "n" of "data" is 1

  Scenario: A group tells when its state changes
    When the property "state" of "sizes" is set to "wide"
    And the property "state" of "sizes" is set to ""
    Then the property "log" of "root" is "[wide][]"

  Scenario: A state of a group is entered while its condition holds
    When the property "on" of "root" is set to true
    Then the property "state" of "shapes" is "tall"
    And the property "height" of "box" is 100

  Scenario: Two groups are in their states independently
    When the property "state" of "sizes" is set to "wide"
    And the property "on" of "root" is set to true
    Then the property "width" of "box" is 100
    And the property "height" of "box" is 100

  Scenario: The item's own state is not a group's state
    When the property "state" of "sizes" is set to "wide"
    Then the property "state" of "root" is ""
