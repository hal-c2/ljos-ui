# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-timeline-keyframegroup.html
#   https://doc.qt.io/qt-6/qml-qtquick-timeline-keyframe.html
# Not yet verified on Qt: the module is not installed beside qml6 on the
# reference machine. The expectations come from the documentation.

@ljos-only
Feature: What a keyframe group can hold
  A group names a property, or one member of a value, and its keyframes hold
  values of the property's type, written or read from a file.

  Scenario: A group's property may be one member of a value
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Timeline
      Item {
        id: root; objectName: "root"
        property vector3d turn: Qt.vector3d(1, 2, 3)
        Timeline {
          id: line; objectName: "line"; endFrame: 10; enabled: true
          KeyframeGroup { target: root; property: "turn.x"; Keyframe { frame: 10; value: 11 } }
          KeyframeGroup { target: root; property: "turn.z"; Keyframe { frame: 10; value: 33 } }
        }
        property real x1: turn.x
        property real y1: turn.y
        property real z1: turn.z
      }
      """
    When the property "currentFrame" of "line" is set to 5
    Then the property "x1" of "root" is 6
    And the property "y1" of "root" is 2
    And the property "z1" of "root" is 18

  Scenario Outline: Values of each type go between keyframes
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Timeline
      Item {
        id: root; objectName: "root"
        property color c: "#000000"
        property point p
        property vector3d v
        property quaternion q
        Timeline {
          id: line; objectName: "line"; endFrame: 10; enabled: true; currentFrame: 10
          KeyframeGroup { target: root; property: "c"; Keyframe { frame: 10; value: "#ff0000" } }
          KeyframeGroup { target: root; property: "p"; Keyframe { frame: 10; value: Qt.point(10, 20) } }
          KeyframeGroup { target: root; property: "v"; Keyframe { frame: 10; value: Qt.vector3d(2, 4, 6) } }
          KeyframeGroup { target: root; property: "q"; Keyframe { frame: 10; value: Qt.quaternion(0, 1, 0, 0) } }
        }
        property string shown: <read>
      }
      """
    Then the property "shown" of "root" is "<expected>"

    Examples:
      | read             | expected |
      | "" + c           | #ff0000  |
      | p.x + "," + p.y  | 10,20    |
      | v.y + "," + v.z  | 4,6      |
      | q.scalar + "," + q.x | 0,1  |

  Scenario: A value may be a binding
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Timeline
      Item {
        id: root; objectName: "root"
        property real end: 10
        property real at: 0
        Timeline {
          id: line; objectName: "line"; endFrame: 10; enabled: true; currentFrame: 10
          KeyframeGroup { target: root; property: "at"; Keyframe { frame: 10; value: root.end } }
        }
      }
      """
    When the property "end" of "root" is set to 30
    Then the property "at" of "root" is 30

  Scenario: A property that does not exist is said
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Timeline
      Item {
        id: root
        Timeline {
          endFrame: 10; enabled: true
          KeyframeGroup { target: root; property: "nothing"; Keyframe { frame: 10; value: 1 } }
        }
      }
      """
    Then the log has a warning matching "Cannot set property"

  Scenario: A keyframe file that cannot be opened is said
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Timeline
      Item {
        id: root
        Timeline {
          endFrame: 10; enabled: true
          KeyframeGroup { target: root; property: "x"; keyframeSource: "missing.qad" }
        }
      }
      """
    Then the log has a warning matching "Unable to open keyframeSource"

  Scenario: A keyframe file that is not one is said
    Given the text file "wrong.qad":
      """
      not keyframes
      """
    And the QML:
      """qml
      import QtQuick
      import QtQuick.Timeline
      Item {
        id: root
        Timeline {
          endFrame: 10; enabled: true
          KeyframeGroup { target: root; property: "x"; keyframeSource: "wrong.qad" }
        }
      }
      """
    Then the log has a warning matching "Corrupt keyframeSource"
