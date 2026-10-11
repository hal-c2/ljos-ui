# Sources:
#   https://doc.qt.io/qt-6/qtquick-statesanimations-states.html
#   https://doc.qt.io/qt-6/qml-qtquick-state.html
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#state-prop
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#states-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: States
  An item has a list of states. Entering one changes what the state names;
  leaving it puts everything back. The state without a name is the base
  state: the item as it is written.

  Rule: A state is entered by its name

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"; width: 200; height: 200
          property string log: ""
          onStateChanged: log += "[" + state + ":" + box.width + "]"
          Rectangle { id: box; objectName: "box"; width: 10; height: 10; color: "#ff0000" }
          states: [
            State { name: "wide"
              PropertyChanges { target: box; width: 100; color: "#0000ff" } },
            State { name: "tall"
              PropertyChanges { target: box; height: 100 } }
          ]
        }
        """

    @api-QtQuick.Item.state
    Scenario: An item starts in the base state
      Then the property "state" of "root" is ""
      And the property "width" of "box" is 10

    @api-QtQuick.Item.states
    Scenario Outline: Entering a state sets the <property> it changes
      When the property "state" of "root" is set to "wide"
      Then the property "<property>" of "box" is <value>

      Examples:
        | property | value     |
        | width    | 100       |
        | color    | "#0000ff" |
        | height   | 10        |

    @api-QtQuick.Item.state
    Scenario Outline: Going back to the base state puts the <property> back
      When the property "state" of "root" is set to "wide"
      And the property "state" of "root" is set to ""
      Then the property "<property>" of "box" is <value>

      Examples:
        | property | value     |
        | width    | 10        |
        | color    | "#ff0000" |

    Scenario Outline: Going from one state to another leaves the first: <property>
      When the property "state" of "root" is set to "wide"
      And the property "state" of "root" is set to "tall"
      Then the property "<property>" of "box" is <value>

      Examples:
        | property | value |
        | width    | 10    |
        | height   | 100   |

    Scenario: A name that no state has is kept as the state
      When the property "state" of "root" is set to "nosuch"
      Then the property "state" of "root" is "nosuch"
      And the log has no warnings

    Scenario: A name that no state has leaves the item as the base state has it
      When the property "state" of "root" is set to "wide"
      And the property "state" of "root" is set to "nosuch"
      Then the property "width" of "box" is 10

    Scenario: The state is told as changed before its changes are made
      When the property "state" of "root" is set to "wide"
      Then the property "log" of "root" is "[wide:10]"

    Scenario: Setting the state an item is in tells of no change
      When the property "state" of "root" is set to "wide"
      And the property "state" of "root" is set to "wide"
      Then the property "log" of "root" is "[wide:10]"

  Rule: A state with a condition is entered while the condition holds

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"; width: 200; height: 200
          property bool wide: false
          property bool tall: false
          Rectangle { id: box; objectName: "box"; width: 10; height: 10 }
          states: [
            State { name: "wide"; when: root.wide
              PropertyChanges { target: box; width: 100 } },
            State { name: "tall"; when: root.tall
              PropertyChanges { target: box; height: 100 } }
          ]
        }
        """

    @api-QtQuick.State.when
    Scenario: The state is entered when its condition becomes true
      When the property "wide" of "root" is set to true
      Then the property "state" of "root" is "wide"
      And the property "width" of "box" is 100

    @api-QtQuick.State.when
    Scenario: The item goes back to the base state when the condition becomes false
      When the property "wide" of "root" is set to true
      And the property "wide" of "root" is set to false
      Then the property "state" of "root" is ""
      And the property "width" of "box" is 10

    Scenario: Of two states whose conditions hold the first in the list is entered
      When the property "tall" of "root" is set to true
      And the property "wide" of "root" is set to true
      Then the property "state" of "root" is "wide"
      And the property "height" of "box" is 10

    Scenario: The second of two states is entered when the first's condition no longer holds
      When the property "tall" of "root" is set to true
      And the property "wide" of "root" is set to true
      And the property "wide" of "root" is set to false
      Then the property "state" of "root" is "tall"
      And the property "height" of "box" is 100

  Rule: An item may be declared in a state

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"; width: 200; height: 200
          property bool on: true
          state: "wide"
          Rectangle { id: box; objectName: "box"; width: 10; height: 10 }
          Item {
            id: other; objectName: "other"
            states: State { name: "on"; when: root.on
              PropertyChanges { target: other; x: 7 } }
          }
          states: State { name: "wide"
            PropertyChanges { target: box; width: 100 } }
        }
        """

    Scenario: An item declared with a state is in it from the start
      Then the property "width" of "box" is 100

    Scenario: A state whose condition holds from the start is entered at the start
      Then the property "state" of "other" is "on"
      And the property "x" of "other" is 7

    Scenario: An item declared with a state can leave it
      When the property "state" of "root" is set to ""
      Then the property "width" of "box" is 10
