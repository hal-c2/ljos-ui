# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-propertyaction.html
#   https://doc.qt.io/qt-6/qml-qtquick-scriptaction.html
#   https://doc.qt.io/qt-6/qml-qtquick-pauseanimation.html
#   https://doc.qt.io/qt-6/qml-qtquick-transition.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Actions in a transition
  In a transition a PropertyAction says when a property takes the state's
  value, a ScriptAction when a script of the state runs, and a
  PauseAnimation lets time pass between them.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 400; height: 400
        property string log: ""
        Rectangle { id: box; objectName: "box"; width: 10; height: 10; color: "#ff0000" }
        states: State { name: "right"
          PropertyChanges { target: box; x: 200; color: "#0000ff"; width: 40; height: 40 }
          StateChangeScript { name: "told"; script: root.log += "told:" + box.color + " " }
          StateChangeScript { script: root.log += "entered:" + box.height + " " } }
        transitions: Transition {
          to: "right"
          SequentialAnimation {
            PropertyAction { target: box; property: "width" }
            PauseAnimation { duration: 1000 }
            PropertyAction { target: box; property: "color" }
            ScriptAction { scriptName: "told" }
            NumberAnimation { target: box; property: "x"; duration: 1000 }
            ScriptAction { script: root.log += "done:" + box.x + " " }
          }
        }
      }
      """

  @api-QtQuick.PropertyAction.property
  Scenario: A PropertyAction at the start gives the property the state's value at once
    When the property "state" of "root" is set to "right"
    Then the property "width" of "box" is 40

  Scenario: A property the transition does not name has the state's value at once
    When the property "state" of "root" is set to "right"
    Then the property "height" of "box" is 40

  Scenario: A property whose PropertyAction comes later keeps its value until then
    When the property "state" of "root" is set to "right"
    And 500 milliseconds pass
    Then the property "color" of "box" is "#ff0000"

  Scenario: A PropertyAction gives the property the state's value at its turn
    When the property "state" of "root" is set to "right"
    And 1500 milliseconds pass
    Then the property "color" of "box" is "#0000ff"

  Scenario: An animation after a pause has not started during the pause
    When the property "state" of "root" is set to "right"
    And 500 milliseconds pass
    Then the property "x" of "box" is 0

  Scenario: An animation after a pause runs after it
    When the property "state" of "root" is set to "right"
    And 1500 milliseconds pass
    Then the property "x" of "box" is at least 20
    And the property "x" of "box" is at most 180

  @api-QtQuick.StateChangeScript.name
  Scenario: A script of the state that the transition names waits for its turn
    When the property "state" of "root" is set to "right"
    And 500 milliseconds pass
    Then the property "log" of "root" is "entered:40 "

  @api-QtQuick.ScriptAction.scriptName
  Scenario: The scripts and actions of a transition run in their order
    When the property "state" of "root" is set to "right"
    And 3 seconds pass
    Then the property "log" of "root" is "entered:40 told:#0000ff done:200 "
