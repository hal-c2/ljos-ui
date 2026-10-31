# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-timeline-timelineanimation.html
#   https://doc.qt.io/qt-6/qml-qtquick-timeline-timeline.html#animations-prop
# Not yet verified on Qt: the module is not installed beside qml6 on the
# reference machine. The expectations come from the documentation.

@ljos-only
Feature: A TimelineAnimation runs its timeline's frames
  It animates the frame number of the timeline it is in. Starting one stops
  the others of that timeline. With pingPong, it goes there and back, and
  each is a run of its own; finished comes when it stops, for whatever reason.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Timeline
      Item {
        id: root; objectName: "root"
        property string log
        Rectangle { id: box; objectName: "box" }
        Timeline {
          objectName: "line"; id: line; endFrame: 100; enabled: true
          animations: [
            TimelineAnimation {
              objectName: "go"; duration: 100; from: 0; to: 100
              onStarted: root.log += "started "
              onStopped: root.log += "stopped "
              onFinished: root.log += "finished "
            },
            TimelineAnimation {
              objectName: "both"; duration: 100; from: 0; to: 100; pingPong: true
              onStarted: root.log += "started "
              onStopped: root.log += "stopped "
              onFinished: root.log += "finished "
            }
          ]
          KeyframeGroup { target: box; property: "x"; Keyframe { frame: 100; value: 100 } }
        }
      }
      """

  Scenario: It moves the frames and so the properties
    When the method "start" of "go" is called
    And 200 milliseconds pass
    Then the property "currentFrame" of "line" is 100
    And the property "x" of "box" is 100
    And the property "log" of "root" is "started stopped finished "

  Scenario: Told to stop it has finished as well
    When the method "start" of "go" is called
    And 20 milliseconds pass
    And the method "stop" of "go" is called
    Then the property "log" of "root" is "started stopped finished "

  Scenario: Starting one stops the other
    When the method "start" of "go" is called
    And 20 milliseconds pass
    And the method "start" of "both" is called
    Then the property "running" of "go" is false
    And the property "running" of "both" is true

  Scenario: There and back is one loop, each way a run of its own
    When the method "start" of "both" is called
    And 400 milliseconds pass
    Then the property "currentFrame" of "line" is 0
    And the property "log" of "root" is "started stopped started stopped finished "
    And the property "from" of "both" is 0
    And the property "to" of "both" is 100

  Scenario: Stopped on the way there, it does not come back
    When the method "start" of "both" is called
    And 20 milliseconds pass
    And the method "stop" of "both" is called
    Then the property "log" of "root" is "started stopped finished "
    And the property "running" of "both" is false
