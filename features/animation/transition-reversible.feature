# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-transition.html#reversible-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Reversible transitions
  A reversible transition also runs for the change the other way round,
  and then runs backwards: what came last comes first.

  Rule: A reversible transition runs backwards for the opposite change

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"; width: 400; height: 400
          property alias going: there.running
          Rectangle { id: box; objectName: "box"; width: 10; height: 10 }
          states: State { name: "away"
            PropertyChanges { target: box; x: 200; y: 200 } }
          transitions: Transition {
            id: there; to: "away"; reversible: true
            SequentialAnimation {
              NumberAnimation { target: box; property: "x"; duration: 2000 }
              NumberAnimation { target: box; property: "y"; duration: 2000 }
            }
          }
        }
        """

    Scenario: On the way there the first animation runs first
      When the property "state" of "root" is set to "away"
      And 1 second passes
      Then the property "x" of "box" is at least 50
      And the property "x" of "box" is at most 150
      And the property "y" of "box" is 0

    Scenario: On the way there the second animation runs second
      When the property "state" of "root" is set to "away"
      And 3 seconds pass
      Then the property "x" of "box" is 200
      And the property "y" of "box" is at least 50
      And the property "y" of "box" is at most 150

    @api-QtQuick.Transition.reversible
    Scenario: On the way back the last animation runs first
      When the property "state" of "root" is set to "away"
      And 5 seconds pass
      And the property "state" of "root" is set to ""
      And 1 second passes
      Then the property "y" of "box" is at least 50
      And the property "y" of "box" is at most 150
      And the property "x" of "box" is 200

    @api-QtQuick.Transition.reversible
    Scenario: On the way back the first animation runs last
      When the property "state" of "root" is set to "away"
      And 5 seconds pass
      And the property "state" of "root" is set to ""
      And 3 seconds pass
      Then the property "y" of "box" is 0
      And the property "x" of "box" is at least 50
      And the property "x" of "box" is at most 150

    @api-QtQuick.Transition.reversible
    Scenario: The way back ends at the values of the state gone back to
      When the property "state" of "root" is set to "away"
      And 5 seconds pass
      And the property "state" of "root" is set to ""
      And 5 seconds pass
      Then the property "x" of "box" is 0
      And the property "y" of "box" is 0
      And the property "going" of "root" is false

  Rule: A transition that is not reversible runs one way only

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"; width: 400; height: 400
          property alias going: there.running
          property bool turns: there.reversible
          Rectangle { id: box; objectName: "box"; width: 10; height: 10 }
          states: State { name: "away"
            PropertyChanges { target: box; x: 200 } }
          transitions: Transition {
            id: there; to: "away"
            NumberAnimation { target: box; property: "x"; duration: 2000 }
          }
        }
        """

    @api-QtQuick.Transition.reversible
    Scenario: A transition is not reversible unless it says so
      Then the property "turns" of "root" is false

    Scenario: The change the other way round is made at once
      When the property "state" of "root" is set to "away"
      And 3 seconds pass
      And the property "state" of "root" is set to ""
      Then the property "x" of "box" is 0
      And the property "going" of "root" is false
