# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-sequentialanimation.html
#   https://doc.qt.io/qt-6/qml-qtquick-parallelanimation.html
#   https://doc.qt.io/qt-6/qml-qtquick-pauseanimation.html
#   https://doc.qt.io/qt-6/qml-qtquick-scriptaction.html
#   https://doc.qt.io/qt-6/qml-qtquick-propertyaction.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Animation groups
  SequentialAnimation runs its children one after the other, ParallelAnimation all at once.
  PauseAnimation waits, ScriptAction runs a script and PropertyAction sets a property at once.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string log
        property alias firstRunning: first.running
        Rectangle { id: box; objectName: "box" }
        SequentialAnimation {
          objectName: "seq"
          onStarted: log += "started "; onStopped: log += "stopped "; onFinished: log += "finished "
          ScriptAction { script: log += "a:" + box.x + " " }
          NumberAnimation {
            id: first; target: box; property: "x"; from: 10; to: 110; duration: 100
            onStarted: log += "child "; onFinished: log += "child "
          }
          PropertyAction { target: box; property: "y"; value: 7 }
          PauseAnimation { duration: 100 }
          ScriptAction { script: log += "b:" + box.x + " " }
        }
        ParallelAnimation {
          objectName: "par"; onFinished: log += "par:" + box.width + " "
          NumberAnimation { target: box; property: "width"; to: 50; duration: 100 }
          NumberAnimation { target: box; property: "height"; to: 80; duration: 300 }
        }
      }
      """

  Scenario: A script at the head of a sequence runs as the sequence starts
    When the method "start" of "seq" is called
    Then the property "log" of "root" is "started a:0 "

  Scenario: The sequence goes on to its next child at once
    When the method "start" of "seq" is called
    Then the property "x" of "box" is 10
    And the property "y" of "box" is 0

  Scenario: The children run in order and the sequence finishes after the last
    When the method "start" of "seq" is called
    And 1 second passes
    Then the property "log" of "root" is "started a:0 b:110 stopped finished "
    And the property "x" of "box" is 110
    And the property "y" of "box" is 7

  Scenario: A child of a group does not report running
    When the method "start" of "seq" is called
    Then the property "firstRunning" of "root" is false

  Scenario: complete() runs every child that is left
    When the method "start" of "seq" is called
    And the method "complete" of "seq" is called
    Then the property "log" of "root" is "started a:0 b:110 stopped finished "
    And the property "y" of "box" is 7

  Scenario: stop() leaves the children that are left undone
    When the method "start" of "seq" is called
    And the method "stop" of "seq" is called
    And 1 second passes
    Then the property "log" of "root" is "started a:0 stopped "
    And the property "y" of "box" is 0

  Scenario: A parallel group ends with its longest child
    When the method "start" of "par" is called
    And 1 second passes
    Then the property "log" of "root" is "par:50 "
    And the property "height" of "box" is 80

  Scenario: A parallel group is still running after its shortest child
    When the method "start" of "par" is called
    And 200 milliseconds pass
    Then the property "running" of "par" is true
    And the property "width" of "box" is 50
