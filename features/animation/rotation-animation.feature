# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-rotationanimation.html
#   https://doc.qt.io/qt-6/qml-qtquick-behavior.html
#   https://doc.qt.io/qt-6/qml-qtquick-animationcontroller.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: RotationAnimation
  direction says which way round an angle turns. The way there may leave the 0 to 360 range; the
  end is always the to value as it was written.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        Rectangle { id: box; objectName: "box" }
        Rectangle {
          id: dial; objectName: "dial"; rotation: 350
          Behavior on rotation {
            RotationAnimation { direction: RotationAnimation.Shortest; duration: 1000 }
          }
        }
        Item { id: gauge; objectName: "gauge"; rotation: 350; property real angle: 350 }
        RotationAnimation { id: anim; objectName: "anim"; target: box; property: "rotation"; duration: 1000 }
        RotationAnimation { objectName: "bare"; target: gauge; to: 10; duration: 1000 }
        RotationAnimation { objectName: "plain"; target: box; to: 90; duration: 1000 }
        AnimationController { id: ctl; animation: anim }
        function turn(direction, from, to, progress) {
          anim.direction = RotationAnimation[direction]
          anim.from = from
          anim.to = to
          ctl.reload()
          ctl.progress = progress
        }
      }
      """

  Scenario: The direction is numerical unless set
    Then the property "direction" of "anim" is 0

  Scenario Outline: <direction> from <from> to <to> is at <quarter>, <half> and <end>
    When the method "turn" of "root" is called with "<direction>", <from>, <to>, 0.25
    Then the property "rotation" of "box" is <quarter>
    When the method "turn" of "root" is called with "<direction>", <from>, <to>, 0.5
    Then the property "rotation" of "box" is <half>
    When the method "turn" of "root" is called with "<direction>", <from>, <to>, 1
    Then the property "rotation" of "box" is <end>

    Examples: Numerical goes straight from one number to the other
      | direction | from | to   | quarter | half | end  |
      | Numerical | 10   | 350  | 95      | 180  | 350  |
      | Numerical | 350  | 10   | 265     | 180  | 10   |
      | Numerical | 0    | 720  | 180     | 360  | 720  |
      | Numerical | 90   | -90  | 45      | 0    | -90  |

    Examples: Clockwise only ever counts up
      | direction | from | to    | quarter | half | end   |
      | Clockwise | 10   | 350   | 95      | 180  | 350   |
      | Clockwise | 350  | 10    | 355     | 360  | 10    |
      | Clockwise | 180  | 0     | 225     | 270  | 0     |
      | Clockwise | 90   | -90   | 135     | 180  | -90   |
      | Clockwise | 0    | 720   | 180     | 360  | 720   |
      | Clockwise | 720  | 0     | 720     | 720  | 0     |
      | Clockwise | 1000 | -1000 | 1040    | 1080 | -1000 |

    Examples: Counterclockwise only ever counts down
      | direction        | from | to    | quarter | half | end   |
      | Counterclockwise | 10   | 350   | 5       | 0    | 350   |
      | Counterclockwise | 350  | 10    | 265     | 180  | 10    |
      | Counterclockwise | 0    | 180   | -45     | -90  | 180   |
      | Counterclockwise | -90  | 90    | -135    | -180 | 90    |
      | Counterclockwise | 0    | 720   | 0       | 0    | 720   |
      | Counterclockwise | -350 | 350   | -355    | -360 | 350   |
      | Counterclockwise | 1000 | -1000 | 500     | 0    | -1000 |

    Examples: Shortest takes the smaller of the two ways round
      | direction | from | to    | quarter | half  | end   |
      | Shortest  | 10   | 350   | 5       | 0     | 350   |
      | Shortest  | 350  | 10    | 355     | 360   | 10    |
      | Shortest  | 0    | 179   | 44.75   | 89.5  | 179   |
      | Shortest  | 0    | 181   | -44.75  | -89.5 | 181   |
      | Shortest  | 0    | 180   | 45      | 90    | 180   |
      | Shortest  | 180  | 0     | 135     | 90    | 0     |
      | Shortest  | 0    | 720   | 0       | 0     | 720   |
      | Shortest  | 370  | 10    | 370     | 370   | 10    |
      | Shortest  | 1000 | -1000 | 1040    | 1080  | -1000 |

  # Qt runs on real time, so a value on the way is only roughly known. The long way round would be
  # near 180 here.
  Scenario: A Behavior turns the short way round
    When the property "rotation" of "dial" is set to 10
    And 500 milliseconds pass
    Then the property "rotation" of "dial" is within 5 of 360
    When 1 second passes
    Then the property "rotation" of "dial" is 10

  Scenario: Without a property it turns rotation and angle
    When the method "start" of "bare" is called
    And 2 seconds pass
    Then the property "rotation" of "gauge" is 10
    And the property "angle" of "gauge" is 10
    And the property "property" of "bare" is ""
    And the property "properties" of "bare" is ""

  Scenario: Without an angle property it turns the rotation alone
    When the method "start" of "plain" is called
    And 2 seconds pass
    Then the property "rotation" of "box" is 90
