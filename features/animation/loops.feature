# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-animation.html#loops-prop
#   https://doc.qt.io/qt-6/qml-qtquick-animation.html#alwaysRunToEnd-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Loops and alwaysRunToEnd
  loops repeats an animation; Animation.Infinite repeats it until it is stopped. alwaysRunToEnd
  lets a stopped animation finish the loop it is in.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string log; property int turns: 0
        property int infinite: Animation.Infinite
        Rectangle { id: box; objectName: "box" }
        NumberAnimation {
          objectName: "twice"; target: box; property: "x"; from: 0; to: 100; duration: 200; loops: 2
          onStopped: log += "stopped "; onFinished: log += "finished "
        }
        NumberAnimation {
          objectName: "forever"; target: box; property: "y"; from: 0; to: 100; duration: 100
          loops: Animation.Infinite; onStopped: log += "stopped "; onFinished: log += "finished "
        }
        SequentialAnimation {
          objectName: "rounds"; loops: 3
          ScriptAction { script: turns++ }
          PauseAnimation { duration: 50 }
        }
        NumberAnimation {
          objectName: "through"; alwaysRunToEnd: true; target: box; property: "width"
          from: 0; to: 100; duration: 200; onStopped: log += "stopped "; onFinished: log += "finished "
        }
        NumberAnimation { objectName: "never"; target: box; property: "height"; to: 9; loops: 0 }
      }
      """

  Scenario: Animation.Infinite is -2
    Then the property "infinite" of "root" is -2

  Scenario: An animation with two loops is still running after its duration
    When the method "start" of "twice" is called
    And 300 milliseconds pass
    Then the property "running" of "twice" is true

  Scenario: An animation with two loops finishes once
    When the method "start" of "twice" is called
    And 1 second passes
    Then the property "log" of "root" is "stopped finished "
    And the property "x" of "box" is 100

  Scenario: An infinite animation keeps running
    When the method "start" of "forever" is called
    And 1 second passes
    Then the property "running" of "forever" is true
    And the property "log" of "root" is ""

  Scenario: Stopping an infinite animation reports finished, then stopped
    When the method "start" of "forever" is called
    And 250 milliseconds pass
    And the method "stop" of "forever" is called
    Then the property "log" of "root" is "finished stopped "
    And the property "running" of "forever" is false

  Scenario: Stopping a looping animation that would end does not report finished
    When the method "start" of "twice" is called
    And the method "stop" of "twice" is called
    Then the property "log" of "root" is "stopped "

  Scenario: A group runs all its children on every loop
    When the method "start" of "rounds" is called
    And 1 second passes
    Then the property "turns" of "root" is 3
    And the property "running" of "rounds" is false

  Scenario: With alwaysRunToEnd, stop() reports not running but lets the animation go on
    When the method "start" of "through" is called
    And the method "stop" of "through" is called
    Then the property "running" of "through" is false
    And the property "log" of "root" is ""

  Scenario: With alwaysRunToEnd, a stopped animation reaches its end and finishes
    When the method "start" of "through" is called
    And the method "stop" of "through" is called
    And 1 second passes
    Then the property "width" of "box" is 100
    And the property "log" of "root" is "stopped finished "

  Scenario: An animation with no loops changes nothing, though it reports running
    When the method "start" of "never" is called
    And 1 second passes
    Then the property "height" of "box" is 0
    And the property "running" of "never" is true
