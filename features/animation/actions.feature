# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-pauseanimation.html
#   https://doc.qt.io/qt-6/qml-qtquick-scriptaction.html
#   https://doc.qt.io/qt-6/qml-qtquick-propertyaction.html
#   https://doc.qt.io/qt-6/qml-qtquick-sequentialanimation.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Actions, pauses and nested groups
  PropertyAction and ScriptAction take no time. They can also be started on their own.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string log
        property int turns: 0
        Rectangle { id: box; objectName: "box" }
        PropertyAction { objectName: "set"; target: box; property: "x"; value: 42 }
        PropertyAction { objectName: "both"; targets: [box, root]; properties: "y,z"; value: 3 }
        ScriptAction { objectName: "say"; script: log += "said "; onFinished: log += "done " }
        PauseAnimation { objectName: "wait"; duration: 100; onFinished: log += "waited " }
        PauseAnimation { objectName: "idle" }
        SequentialAnimation {
          objectName: "nest"; onFinished: log += "end "
          ParallelAnimation {
            NumberAnimation { target: box; property: "width"; to: 20; duration: 50 }
            SequentialAnimation {
              PauseAnimation { duration: 100 }
              ScriptAction { script: log += "inner:" + box.width + " " }
            }
          }
          ScriptAction { script: log += "after " }
        }
        SequentialAnimation { objectName: "empty"; onFinished: log += "empty " }
      }
      """

  Scenario: A PropertyAction that is started sets its value at once
    When the method "start" of "set" is called
    Then the property "x" of "box" is 42
    And the property "running" of "set" is false

  Scenario Outline: A PropertyAction sets "<property>" of "<object>" from targets and properties
    When the method "start" of "both" is called
    Then the property "<property>" of "<object>" is 3

    Examples:
      | object | property |
      | box    | y        |
      | box    | z        |
      | root   | y        |
      | root   | z        |

  Scenario: A ScriptAction that is started runs its script at once and finishes
    When the method "start" of "say" is called
    Then the property "log" of "root" is "said done "

  Scenario: A PauseAnimation lasts 250 milliseconds unless told otherwise
    Then the property "duration" of "idle" is 250

  Scenario: A PauseAnimation on its own runs for its duration
    When the method "start" of "wait" is called
    Then the property "running" of "wait" is true
    And the property "log" of "root" is ""

  Scenario: A PauseAnimation on its own finishes
    When the method "start" of "wait" is called
    And 1 second passes
    Then the property "log" of "root" is "waited "

  Scenario: Groups nest
    When the method "start" of "nest" is called
    And 1 second passes
    Then the property "log" of "root" is "inner:20 after end "

  Scenario: An empty group finishes as it starts
    When the method "start" of "empty" is called
    Then the property "log" of "root" is "empty "
    And the property "running" of "empty" is false
