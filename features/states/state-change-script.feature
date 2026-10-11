# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-statechangescript.html
#   https://doc.qt.io/qt-6/qml-qtquick-state.html#changes-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: StateChangeScript
  A StateChangeScript is a statement that runs when its state is entered,
  at its place among the state's changes.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 200; height: 200
        property string log: ""
        Rectangle { id: box; objectName: "box"; width: 10; height: 10 }
        states: [
          State { name: "wide"
            StateChangeScript { script: root.log += "first:" + box.width + " " }
            PropertyChanges { target: box; width: 60 }
            StateChangeScript { name: "second"; script: root.log += "second:" + box.width + " " }
          }
        ]
      }
      """

  @api-QtQuick.StateChangeScript.script
  Scenario: The scripts run in their places among the changes
    When the property "state" of "root" is set to "wide"
    Then the property "log" of "root" is "first:10 second:60 "

  Scenario: A script does not run before its state is entered
    Then the property "log" of "root" is ""

  Scenario: A script does not run when its state is left
    When the property "state" of "root" is set to "wide"
    And the property "state" of "root" is set to ""
    Then the property "log" of "root" is "first:10 second:60 "

  Scenario: The scripts run each time the state is entered
    When the property "state" of "root" is set to "wide"
    And the property "state" of "root" is set to ""
    And the property "state" of "root" is set to "wide"
    Then the property "log" of "root" is "first:10 second:60 first:10 second:60 "
