# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-animation.html#running-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: An animation whose running property is bound
  A binding on running starts and stops the animation, and the animation ending by itself does not
  break the binding.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property bool go: false
        property int runs: 0
        Rectangle { id: box; objectName: "box" }
        NumberAnimation {
          objectName: "anim"
          running: go
          target: box; property: "x"; from: 10; to: 110; duration: 100
          onStarted: runs++
        }
      }
      """

  Scenario: The animation starts when the binding becomes true
    When the property "go" of "root" is set to true
    Then the property "x" of "box" is 10
    And the property "running" of "anim" is true

  Scenario: The animation stops when the binding becomes false
    When the property "go" of "root" is set to true
    And the property "go" of "root" is set to false
    Then the property "running" of "anim" is false

  Scenario: Ending by itself keeps the binding
    When the property "go" of "root" is set to true
    And 1 second passes
    And the property "go" of "root" is set to false
    And the property "go" of "root" is set to true
    Then the property "running" of "anim" is true
    And the property "runs" of "root" is 2

  Scenario: stop() keeps the binding
    When the property "go" of "root" is set to true
    And the method "stop" of "anim" is called
    And the property "go" of "root" is set to false
    And the property "go" of "root" is set to true
    Then the property "running" of "anim" is true
    And the property "runs" of "root" is 2
