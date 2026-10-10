# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-animation.html
#   https://doc.qt.io/qt-6/qtquick-statesanimations-animations.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Starting and stopping an animation
  An animation runs from start() until it reaches its end or is stopped, and reports each step
  through running, paused and the started, stopped and finished signals.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string log: ""
        property real seen: -1
        Rectangle { id: box; objectName: "box" }
        NumberAnimation {
          objectName: "anim"
          target: box; property: "x"; from: 10; to: 110; duration: 200
          onStarted: { log += "started "; seen = box.x }
          onStopped: log += "stopped "
          onFinished: log += "finished "
          onRunningChanged: log += running ? "on " : "off "
          onPausedChanged: log += paused ? "paused " : "resumed "
        }
      }
      """

  Scenario: An animation does nothing until it is started
    When 300 milliseconds pass
    Then the property "x" of "box" is 0
    And the property "running" of "anim" is false
    And the property "log" of "root" is ""

  Scenario: start() puts the property at the start value at once
    When the method "start" of "anim" is called
    Then the property "x" of "box" is 10
    And the property "running" of "anim" is true

  Scenario: started is emitted before the property moves and before running is reported
    When the method "start" of "anim" is called
    Then the property "seen" of "root" is 0
    And the property "log" of "root" is "started on "

  Scenario: The animation ends at the end value, then reports stopped and finished
    When the method "start" of "anim" is called
    And 1 second passes
    Then the property "x" of "box" is 110
    And the property "running" of "anim" is false
    And the property "log" of "root" is "started on stopped off finished "

  Scenario: Starting an animation that runs changes nothing
    When the method "start" of "anim" is called
    And the method "start" of "anim" is called
    Then the property "log" of "root" is "started on "

  Scenario: stop() leaves the property where it is and does not emit finished
    When the method "start" of "anim" is called
    And the method "stop" of "anim" is called
    And 300 milliseconds pass
    Then the property "x" of "box" is 10
    And the property "running" of "anim" is false
    And the property "log" of "root" is "started on stopped off "

  Scenario: complete() jumps to the end value and finishes
    When the method "start" of "anim" is called
    And the method "complete" of "anim" is called
    Then the property "x" of "box" is 110
    And the property "log" of "root" is "started on stopped off finished "

  Scenario: complete() does nothing to an animation that is not running
    When the method "complete" of "anim" is called
    Then the property "x" of "box" is 0
    And the property "log" of "root" is ""

  Scenario: restart() stops a running animation and starts it again
    When the method "start" of "anim" is called
    And the method "restart" of "anim" is called
    Then the property "log" of "root" is "started on stopped off started on "
    And the property "running" of "anim" is true

  Scenario: restart() starts an animation that is not running
    When the method "restart" of "anim" is called
    Then the property "log" of "root" is "started on "
    And the property "x" of "box" is 10

  Scenario: A paused animation keeps running but stands still
    When the method "start" of "anim" is called
    And the method "pause" of "anim" is called
    And 300 milliseconds pass
    Then the property "x" of "box" is 10
    And the property "running" of "anim" is true
    And the property "paused" of "anim" is true
    And the property "log" of "root" is "started on paused "

  Scenario: resume() lets a paused animation go on to its end
    When the method "start" of "anim" is called
    And the method "pause" of "anim" is called
    And 300 milliseconds pass
    And the method "resume" of "anim" is called
    And 1 second passes
    Then the property "x" of "box" is 110
    And the property "log" of "root" is "started on paused resumed stopped off finished "

  Scenario: Stopping a paused animation ends the pause first
    When the method "start" of "anim" is called
    And the method "pause" of "anim" is called
    And the method "stop" of "anim" is called
    Then the property "paused" of "anim" is false
    And the property "log" of "root" is "started on paused resumed stopped off "

  Scenario: An animation that is not running cannot be paused
    When the method "pause" of "anim" is called
    Then the property "paused" of "anim" is false
    And the log has a warning matching "setPaused\(\) cannot be used when animation isn't running"

  Scenario: resume() does nothing to an animation that is not running
    When the method "resume" of "anim" is called
    Then the property "log" of "root" is ""
    And the log has no warnings

  Scenario: Setting running to true starts the animation
    When the property "running" of "anim" is set to true
    Then the property "x" of "box" is 10
    And the property "log" of "root" is "started on "

  Scenario: Setting running to false stops the animation
    When the method "start" of "anim" is called
    And the property "running" of "anim" is set to false
    Then the property "log" of "root" is "started on stopped off "

  Scenario: Setting paused to true pauses a running animation
    When the method "start" of "anim" is called
    And the property "paused" of "anim" is set to true
    And 300 milliseconds pass
    Then the property "x" of "box" is 10
    And the property "running" of "anim" is true
