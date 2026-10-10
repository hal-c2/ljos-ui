# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-smoothedanimation.html
#   https://doc.qt.io/qt-6/qml-qtquick-animation.html
#   https://doc.qt.io/qt-6/qml-qtquick-animationcontroller.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: SmoothedAnimation outside a Behavior
  Started by hand, a SmoothedAnimation goes from where the property is to its to value. It differs
  from a NumberAnimation in several ways that the Qt documentation leaves out.

  # Qt runs on real time here, so values on the way are given with a tolerance.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property real slide: 0
        property real round: 0
        property int steps: 0
        property real held: 0
        property string log
        SmoothedAnimation {
          objectName: "mover"; target: root; property: "slide"; from: 50; to: 100; velocity: 100
          onStarted: log += "started "; onStopped: log += "stopped "; onFinished: log += "finished "
        }
        SmoothedAnimation {
          objectName: "endless"; target: root; property: "round"; to: 20; loops: Animation.Infinite
        }
        SmoothedAnimation { objectName: "stepper"; target: root; property: "steps"; to: 7; velocity: 10 }
        SmoothedAnimation { id: led; target: root; property: "held"; to: 100 }
        AnimationController { objectName: "hand"; animation: led }
      }
      """

  Scenario: The from value is not used
    When the method "start" of "mover" is called
    And 300 milliseconds pass
    Then the property "slide" of "root" is within 6 of 18

  Scenario: It arrives at the to value
    When the method "start" of "mover" is called
    And 1.5 seconds pass
    Then the property "slide" of "root" is 100
    And the property "running" of "mover" is false
    And the property "log" of "root" is "started stopped finished "

  Scenario: Stopped on the way it reports finished as well
    When the method "start" of "mover" is called
    And 300 milliseconds pass
    And the method "stop" of "mover" is called
    Then the property "log" of "root" is "started finished stopped "
    And the property "slide" of "root" is at most 40

  Scenario: Restarted on the way it reports finished, stopped and started
    When the method "start" of "mover" is called
    And 300 milliseconds pass
    And the method "restart" of "mover" is called
    Then the property "log" of "root" is "started finished stopped started "

  Scenario: Restarted on the way it keeps its speed
    When the method "start" of "mover" is called
    And 500 milliseconds pass
    And the method "restart" of "mover" is called
    And 100 milliseconds pass
    Then the property "slide" of "root" is within 8 of 70

  Scenario: A new to value on the way changes nothing
    When the method "start" of "mover" is called
    And 300 milliseconds pass
    And the property "to" of "mover" is set to 0
    And 1.5 seconds pass
    Then the property "slide" of "root" is 100

  Scenario: A value written on the way does not last
    When the method "start" of "mover" is called
    And 300 milliseconds pass
    And the property "slide" of "root" is set to 90
    And 100 milliseconds pass
    Then the property "slide" of "root" is at most 45

  Scenario: Loops add nothing, not even without end
    When the method "start" of "endless" is called
    And 1 second passes
    Then the property "round" of "root" is 20
    And the property "running" of "endless" is false

  Scenario: A whole-number property is moved in whole numbers
    When the method "start" of "stepper" is called
    And 350 milliseconds pass
    Then the property "steps" of "root" is at least 2
    And the property "steps" of "root" is at most 5
    When 1 second passes
    Then the property "steps" of "root" is 7

  Scenario: An AnimationController does not move it
    When the property "progress" of "hand" is set to 0.5
    Then the property "held" of "root" is 0
    When the property "progress" of "hand" is set to 1
    Then the property "held" of "root" is 0
