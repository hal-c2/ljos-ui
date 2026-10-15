# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-animation.html#loops-prop
#   https://doc.qt.io/qt-6/qml-qtquick-animation.html#running-prop
# Verified on Qt 6.11.2 (qml6, offscreen): an animation that takes no time
# is over as soon as it starts, whatever loops says, and runs once.

@qt-parity
Feature: An animation that takes no time
  An animation whose duration is 0, or a group of such animations, has
  nothing to wait for. Starting it puts its properties at their end, runs
  its scripts once and stops it there and then, even with loops set to
  Animation.Infinite, so it never keeps the application busy.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string log; property int turns: 0; property int rounds: 0
        Rectangle { id: box; objectName: "box" }
        NumberAnimation {
          objectName: "forever"; target: box; property: "x"; to: 90; duration: 0
          loops: Animation.Infinite
          onStarted: log += "started "; onStopped: log += "stopped "; onFinished: log += "finished "
        }
        SequentialAnimation {
          objectName: "turning"; loops: Animation.Infinite
          ScriptAction { script: turns++ }
        }
        SequentialAnimation {
          objectName: "thrice"; loops: 3
          ScriptAction { script: rounds++ }
          PauseAnimation { duration: 0 }
        }
        PauseAnimation { objectName: "rest"; duration: 0; loops: Animation.Infinite }
        Rectangle {
          objectName: "mover"; property bool moving: source.running
          NumberAnimation on y { id: source; to: 50; duration: 0; loops: Animation.Infinite }
        }
      }
      """

  Scenario: An endless animation of no length ends where it is going as it starts
    When the method "start" of "forever" is called
    Then the property "x" of "box" is 90
    And the property "running" of "forever" is false
    And the property "log" of "root" is "started stopped finished "

  Scenario: An endless group of no length runs its script once
    When the method "start" of "turning" is called
    And 100 milliseconds pass
    Then the property "turns" of "root" is 1
    And the property "running" of "turning" is false

  Scenario: A group of no length runs its script once, however many loops it has
    When the method "start" of "thrice" is called
    And 100 milliseconds pass
    Then the property "rounds" of "root" is 1
    And the property "running" of "thrice" is false

  Scenario: An endless pause of no length is over as it starts
    When the method "start" of "rest" is called
    Then the property "running" of "rest" is false

  Scenario: An endless value source of no length leaves its property at the end
    Then the property "y" of "mover" is 50
    And the property "moving" of "mover" is false
