# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-object3d.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation.

Feature: A node has states, as an item has
  Every object of a scene has `states`, `state` and `transitions`. A state's
  changes apply to the object, its `when` picks it and a transition animates
  the change.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick3D
      Item {
        View3D {
          PointLight {
            id: lamp
            objectName: "lamp"
            brightness: 0
            states: [
              State { name: "On"; when: flag.on; PropertyChanges { target: lamp; brightness: 10 } }
            ]
          }
          Node {
            id: mover
            objectName: "mover"
            states: State { name: "away"; PropertyChanges { target: mover; x: 40 } }
            transitions: Transition { NumberAnimation { property: "x"; duration: 200 } }
          }
        }
        QtObject { id: flag; objectName: "flag"; property bool on: false }
      }
      """

  Scenario: A state's when changes the light it belongs to
    When the property "on" of "flag" is set to true
    Then the property "state" of "lamp" is "On"
    And the property "brightness" of "lamp" is 10

  Scenario: Leaving the state puts the value back
    When the property "on" of "flag" is set to true
    And the property "on" of "flag" is set to false
    Then the property "state" of "lamp" is ""
    And the property "brightness" of "lamp" is 0

  Scenario: A transition animates the change of a node
    When the property "state" of "mover" is set to "away"
    Then the property "x" of "mover" is 0
    When 300 milliseconds pass
    Then the property "x" of "mover" is 40
