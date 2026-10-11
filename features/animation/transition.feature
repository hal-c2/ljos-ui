# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-transition.html
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#transitions-prop
#   https://doc.qt.io/qt-6/qtquick-statesanimations-animations.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Transitions between states
  A Transition animates the properties a state change gives new values.
  What it does not animate has its new value at once.

  Rule: A transition takes the properties it names to the state's values

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"; width: 400; height: 400
          property string log: ""
          property alias moving: move.running
          property alias sliding: slide.running
          Rectangle { id: box; objectName: "box"; width: 10; height: 10 }
          Rectangle { id: other; objectName: "other"; width: 10; height: 10 }
          states: State { name: "right"
            PropertyChanges { target: box; x: 200; y: 50; width: 40 }
            PropertyChanges { target: other; x: 100 } }
          transitions: Transition {
            id: slide
            onRunningChanged: root.log += running ? "on " : "off "
            NumberAnimation { id: move; properties: "x"; duration: 2000 }
          }
        }
        """

    @api-QtQuick.Item.transitions
    Scenario: A property the transition animates starts where it was
      When the property "state" of "root" is set to "right"
      Then the property "x" of "box" is 0

    Scenario Outline: A property the transition does not animate has its new value at once: <property>
      When the property "state" of "root" is set to "right"
      Then the property "<property>" of "box" is <value>

      Examples:
        | property | value |
        | y        | 50    |
        | width    | 40    |

    Scenario: The animated property is on its way while the transition runs
      When the property "state" of "root" is set to "right"
      And 1 second passes
      Then the property "x" of "box" is at least 50
      And the property "x" of "box" is at most 150

    Scenario: The animated property arrives at the state's value
      When the property "state" of "root" is set to "right"
      And 3 seconds pass
      Then the property "x" of "box" is 200

    Scenario: An animation that names no target animates every object the state changes
      When the property "state" of "root" is set to "right"
      And 1 second passes
      Then the property "x" of "other" is at least 25
      And the property "x" of "other" is at most 75

    @api-QtQuick.Transition.running
    Scenario: A transition is running while its animations run
      When the property "state" of "root" is set to "right"
      Then the property "sliding" of "root" is true

    @api-QtQuick.Transition.running
    Scenario: A transition is no longer running when its animations are over
      When the property "state" of "root" is set to "right"
      And 3 seconds pass
      Then the property "sliding" of "root" is false
      And the property "log" of "root" is "on off "

    Scenario: An animation of a transition does not report that it runs
      When the property "state" of "root" is set to "right"
      Then the property "moving" of "root" is false

    Scenario: Without a `from` or a `to` a transition runs on the way back as well
      When the property "state" of "root" is set to "right"
      And 3 seconds pass
      And the property "state" of "root" is set to ""
      And 1 second passes
      Then the property "x" of "box" is at least 50
      And the property "x" of "box" is at most 150

    Scenario: The way back ends at the value of the base state
      When the property "state" of "root" is set to "right"
      And 3 seconds pass
      And the property "state" of "root" is set to ""
      And 3 seconds pass
      Then the property "x" of "box" is 0

  Rule: An animation's own values count before the state's

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"; width: 400; height: 400
          Rectangle { id: box; objectName: "box"; width: 10; height: 10; color: "#ff0000" }
          states: State { name: "right"
            PropertyChanges { target: box; x: 200; y: 200; width: 100; color: "#0000ff" } }
          transitions: Transition {
            NumberAnimation { target: box; property: "x"; from: 50; duration: 2000 }
            NumberAnimation { target: box; property: "y"; from: 10; to: 20; duration: 2000 }
            NumberAnimation { target: box; property: "width"; to: 30; duration: 4000 }
            ColorAnimation { duration: 2000 }
          }
        }
        """

    Scenario: An animation with a `from` starts there
      When the property "state" of "root" is set to "right"
      Then the property "x" of "box" is 50

    Scenario: An animation with a `from` still ends at the state's value
      When the property "state" of "root" is set to "right"
      And 5 seconds pass
      Then the property "x" of "box" is 200

    Scenario: An animation with a `to` goes there while the transition runs
      When the property "state" of "root" is set to "right"
      And 1 second passes
      Then the property "y" of "box" is at least 11
      And the property "y" of "box" is at most 19

    Scenario: The state's value is what the property has when the transition is over
      When the property "state" of "root" is set to "right"
      And 5 seconds pass
      Then the property "y" of "box" is 200
      And the property "width" of "box" is 100

    Scenario: The state's value waits for the whole transition, not for the one animation
      When the property "state" of "root" is set to "right"
      And 3 seconds pass
      Then the property "y" of "box" is 20

    Scenario: A ColorAnimation that names nothing animates the colors the state changes
      When the property "state" of "root" is set to "right"
      Then the property "color" of "box" is "#ff0000"

    Scenario: The color arrives at the state's value
      When the property "state" of "root" is set to "right"
      And 5 seconds pass
      Then the property "color" of "box" is "#0000ff"

  Rule: Only a change of state is animated

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"; width: 400; height: 400
          state: "right"
          property int far: 200
          property alias sliding: slide.running
          Rectangle { id: box; objectName: "box"; width: 10; height: 10 }
          states: State { name: "right"
            PropertyChanges { target: box; x: root.far } }
          transitions: Transition {
            id: slide
            NumberAnimation { properties: "x"; duration: 2000 }
          }
        }
        """

    Scenario: The state an item is declared in is entered without a transition
      Then the property "x" of "box" is 200
      And the property "sliding" of "root" is false

    Scenario: A new value of a state's binding is not animated
      When the property "far" of "root" is set to 300
      Then the property "x" of "box" is 300
