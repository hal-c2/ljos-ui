# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-timeline-timeline.html
#   https://doc.qt.io/qt-6/qml-qtquick-timeline-keyframegroup.html
#   https://doc.qt.io/qt-6/qml-qtquick-timeline-keyframe.html
# Not yet verified on Qt: the module is not installed beside qml6 on the
# reference machine. The expectations come from the documentation.

@ljos-only
Feature: A timeline gives properties the values of its keyframes
  An enabled Timeline holds the properties its keyframe groups name and gives
  them what its current frame says. A keyframe's easing is the way to it; past
  the last keyframe the property stays at the last value. Disabled, a property
  is what it was.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Timeline
      Item {
        id: root; objectName: "root"
        property real follows: 7
        Rectangle { id: box; objectName: "box"; x: 5; y: root.follows }
        Timeline {
          id: line; objectName: "line"; startFrame: 0; endFrame: 100; enabled: true
          KeyframeGroup {
            target: box; property: "x"
            Keyframe { frame: 50; value: 100 }
            Keyframe { frame: 100; value: 200 }
          }
          KeyframeGroup {
            target: box; property: "y"
            Keyframe { frame: 100; value: 40; easing.type: Easing.InQuad }
            Keyframe { frame: 20; value: 20 }
          }
        }
      }
      """

  Scenario: Between two keyframes the value is on the way
    When the property "currentFrame" of "line" is set to 75
    Then the property "x" of "box" is 150

  Scenario: Before the first keyframe the way starts at what the property was
    When the property "currentFrame" of "line" is set to 25
    Then the property "x" of "box" is 52.5

  Scenario: Keyframes count in order of frame, as written or not
    When the property "currentFrame" of "line" is set to 20
    Then the property "y" of "box" is 20

  Scenario: The easing of a keyframe is the way to it
    When the property "currentFrame" of "line" is set to 60
    Then the property "y" of "box" is 25

  Scenario: Past the last keyframe the value is the last's
    When the property "currentFrame" of "line" is set to 500
    Then the property "x" of "box" is 200

  Scenario: Disabling gives back the value
    When the property "currentFrame" of "line" is set to 75
    And the property "enabled" of "line" is set to false
    Then the property "x" of "box" is 5

  Scenario: Disabling gives back the binding, which went on underneath
    When the property "currentFrame" of "line" is set to 60
    And the property "follows" of "root" is set to 9
    And the property "enabled" of "line" is set to false
    Then the property "y" of "box" is 9
    When the property "follows" of "root" is set to 11
    Then the property "y" of "box" is 11

  Scenario: A disabled timeline leaves the properties alone
    When the property "enabled" of "line" is set to false
    And the property "currentFrame" of "line" is set to 75
    Then the property "x" of "box" is 5

  Scenario: Enabling again shows the frame it is at
    When the property "enabled" of "line" is set to false
    And the property "currentFrame" of "line" is set to 75
    And the property "enabled" of "line" is set to true
    Then the property "x" of "box" is 150
