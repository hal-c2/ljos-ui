# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-behavior.html
#   https://doc.qt.io/qt-6/qtquick-statesanimations-behaviors.html
#   https://doc.qt.io/qt-6/qtquick-statesanimations-animations.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Behavior
  A Behavior animates every change of the property it is declared on.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property real goal: 30
        property string log
        property alias smooth: glide.enabled
        property alias gliding: move.running
        property var aim: glide.targetValue
        Rectangle {
          id: box; objectName: "box"
          x: 100; y: root.goal; width: 10; color: "#000000"
          Behavior on x {
            id: glide
            NumberAnimation {
              id: move; duration: 2000
              onRunningChanged: log += running ? "on " : "off "
              onStarted: log += "started "; onFinished: log += "finished "
            }
          }
          Behavior on y { NumberAnimation { duration: 100 } }
          Behavior on width { }
          Behavior on color { ColorAnimation { duration: 100 } }
        }
      }
      """

  Scenario: The first value is not animated
    Then the property "x" of "box" is 100
    And the property "y" of "box" is 30
    And the property "gliding" of "root" is false
    And the property "log" of "root" is ""

  Scenario: A new value is not taken at once
    When the property "x" of "box" is set to 200
    Then the property "x" of "box" is 100
    And the property "gliding" of "root" is true

  Scenario: The property arrives at the new value
    When the property "x" of "box" is set to 200
    And 3 seconds pass
    Then the property "x" of "box" is 200
    And the property "gliding" of "root" is false

  Scenario: The animation of a Behavior reports running but not started or finished
    When the property "x" of "box" is set to 200
    And 3 seconds pass
    Then the property "log" of "root" is "on off "

  Scenario: A new value on the way starts from where the property is
    When the property "x" of "box" is set to 200
    And 1 second passes
    And the property "x" of "box" is set to 100
    Then the property "x" of "box" is at least 130
    And the property "x" of "box" is at most 170

  Scenario: A new value on the way takes the whole duration again
    When the property "x" of "box" is set to 200
    And 1 second passes
    And the property "x" of "box" is set to 100
    And 1 second passes
    Then the property "x" of "box" is at least 110
    And the property "gliding" of "root" is true

  Scenario: A change that comes from a binding is animated
    When the property "goal" of "root" is set to 80
    Then the property "y" of "box" is 30

  Scenario: The property stays bound after the animation
    When the property "goal" of "root" is set to 80
    And 1 second passes
    And the property "goal" of "root" is set to 60
    And 1 second passes
    Then the property "y" of "box" is 60

  Scenario: A disabled Behavior lets the value through
    When the property "smooth" of "root" is set to false
    And the property "x" of "box" is set to 200
    Then the property "x" of "box" is 200
    And the property "gliding" of "root" is false

  Scenario: A value written while disabled is not overtaken by the animation that was running
    When the property "x" of "box" is set to 200
    And 500 milliseconds pass
    And the property "smooth" of "root" is set to false
    And the property "x" of "box" is set to 300
    And 3 seconds pass
    Then the property "x" of "box" is 300

  Scenario: A Behavior without an animation lets the value through
    When the property "width" of "box" is set to 40
    Then the property "width" of "box" is 40

  Scenario: targetValue is undefined before the first change
    Then the property "aim" of "root" is null

  Scenario: targetValue is the value the property is going to
    When the property "x" of "box" is set to 200
    Then the property "aim" of "root" is 200

  Scenario: A colour is animated by a ColorAnimation
    When the property "color" of "box" is set to "#ff0000"
    Then the property "color" of "box" is "#000000"

  Scenario: The colour arrives
    When the property "color" of "box" is set to "#ff0000"
    And 1 second passes
    Then the property "color" of "box" is "#ff0000"

  Scenario: Writing the value the property already has starts nothing
    When the property "x" of "box" is set to 100
    Then the property "gliding" of "root" is false
