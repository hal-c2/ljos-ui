# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-animation.html
#   https://doc.qt.io/qt-6/qml-qtquick-propertyanimation.html
#   https://doc.qt.io/qt-6/qml-qtquick-behavior.html
#   https://doc.qt.io/qt-6/qml-qtquick-sequentialanimation.html

# The Qt oracle waits in real time, so it cannot stop at an exact instant of an animation. These
# scenarios check the values in between on the application's own clock.
@ljos-only
Feature: Animation values at exact times
  At any instant, a running animation shows the value its easing curve gives for the time passed.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        Rectangle {
          id: box; objectName: "box"; x: 20
          Behavior on y { NumberAnimation { duration: 100 } }
        }
        Item { id: page; objectName: "page"; visible: false; Item { id: far; objectName: "far" } }
        NumberAnimation { objectName: "anim"; target: box; property: "x"; from: 0; to: 100; duration: 200 }
        NumberAnimation { objectName: "loop"; target: box; property: "x"; to: 100; duration: 100; loops: 2 }
        NumberAnimation {
          objectName: "eased"; target: box; property: "x"; from: 0; to: 100; duration: 200
          easing.type: Easing.InQuad
        }
        SequentialAnimation {
          objectName: "seq"
          NumberAnimation { target: box; property: "width"; from: 0; to: 100; duration: 100 }
          PauseAnimation { duration: 100 }
          NumberAnimation { target: box; property: "width"; to: 0; duration: 100 }
        }
        NumberAnimation { objectName: "unseen"; target: far; property: "x"; to: 100; duration: 200 }
        NumberAnimation {
          objectName: "round"; target: far; property: "y"; from: 0; to: 100; duration: 200; loops: -2
        }
      }
      """

  Scenario Outline: After <time> milliseconds of 200 the value is <x>
    When the method "start" of "anim" is called
    And <time> milliseconds pass
    Then the property "x" of "box" is <x>

    Examples:
      | time | x   |
      | 50   | 25  |
      | 100  | 50  |
      | 150  | 75  |
      | 200  | 100 |
      | 250  | 100 |

  Scenario: The easing curve shapes the values in between
    When the method "start" of "eased" is called
    And 100 milliseconds pass
    Then the property "x" of "box" is 25

  Scenario: A paused animation does not move
    When the method "start" of "anim" is called
    And 50 milliseconds pass
    And the method "pause" of "anim" is called
    And 500 milliseconds pass
    Then the property "x" of "box" is 25

  Scenario: A resumed animation goes on from where it paused
    When the method "start" of "anim" is called
    And 50 milliseconds pass
    And the method "pause" of "anim" is called
    And 500 milliseconds pass
    And the method "resume" of "anim" is called
    And 50 milliseconds pass
    Then the property "x" of "box" is 50

  Scenario: A stopped animation leaves the value it reached
    When the method "start" of "anim" is called
    And 100 milliseconds pass
    And the method "stop" of "anim" is called
    And 100 milliseconds pass
    Then the property "x" of "box" is 50

  Scenario: restart() begins again from the start value
    When the method "start" of "anim" is called
    And 150 milliseconds pass
    And the method "restart" of "anim" is called
    And 50 milliseconds pass
    Then the property "x" of "box" is 25

  Scenario: A change of to does not reach the run that is under way
    When the method "start" of "anim" is called
    And 100 milliseconds pass
    And the property "to" of "anim" is set to 500
    And 50 milliseconds pass
    Then the property "x" of "box" is 75

  Scenario Outline: A looping animation without from starts each loop at the same value (<time> ms)
    When the method "start" of "loop" is called
    And <time> milliseconds pass
    Then the property "x" of "box" is <x>

    Examples:
      | time | x   |
      | 50   | 60  |
      | 150  | 60  |
      | 200  | 100 |

  Scenario Outline: A sequence after <time> milliseconds
    When the method "start" of "seq" is called
    And <time> milliseconds pass
    Then the property "width" of "box" is <width>
    And the property "running" of "seq" is <running>

    Examples:
      | time | width | running |
      | 50   | 50    | true    |
      | 150  | 100   | true    |
      | 250  | 50    | true    |
      | 300  | 0     | false   |

  Scenario: A Behavior that gets a new value on the way starts again from the value it reached
    When the property "y" of "box" is set to 100
    And 50 milliseconds pass
    And the property "y" of "box" is set to 0
    And 50 milliseconds pass
    Then the property "y" of "box" is 25

  Scenario: An item shown in the middle of its animation is where an animation in view would be
    When the method "start" of "unseen" is called
    And 100 milliseconds pass
    And the property "visible" of "page" is set to true
    Then the property "x" of "far" is 50

  Scenario: An item shown again goes on to the end
    When the method "start" of "unseen" is called
    And 100 milliseconds pass
    And the property "visible" of "page" is set to true
    And 50 milliseconds pass
    Then the property "x" of "far" is 75

  Scenario: The value of a hidden item's property can be read in the middle of its animation
    When the method "start" of "unseen" is called
    And 150 milliseconds pass
    Then the property "x" of "far" is 75

  Scenario: An endless animation shown again is in step with the clock
    When the method "start" of "round" is called
    And 1050 milliseconds pass
    And the property "visible" of "page" is set to true
    Then the property "y" of "far" is 25
