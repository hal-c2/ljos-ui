# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-animator.html
#   https://doc.qt.io/qt-6/qml-qtquick-opacityanimator.html
#   https://doc.qt.io/qt-6/qml-qtquick-xanimator.html
#   https://doc.qt.io/qt-6/qml-qtquick-yanimator.html
#   https://doc.qt.io/qt-6/qml-qtquick-scaleanimator.html
#   https://doc.qt.io/qt-6/qml-qtquick-rotationanimator.html
#   https://doc.qt.io/qt-6/qml-qtquick-uniformanimator.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Animators
  An animator moves one property of its target the way a NumberAnimation
  would, but only the picture moves while it runs: the property keeps its
  old value and is written once, when the animator ends.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 400; height: 400
        property string log
        property int changes: 0
        Rectangle {
          id: box; objectName: "box"; width: 50; height: 50; y: 150
          onOpacityChanged: root.changes++; onXChanged: root.changes++
          onScaleChanged: root.changes++; onRotationChanged: root.changes++
          Behavior on x { XAnimator { duration: 400 } }
        }
        OpacityAnimator {
          objectName: "fade"; target: box; from: 1; to: 0.2; duration: 400
          onStarted: root.log += "S"; onStopped: root.log += "T"; onFinished: root.log += "F"
          onRunningChanged: root.log += running ? "r" : "n"
        }
        OpacityAnimator { objectName: "rise"; target: box; to: 1; duration: 400 }
        YAnimator { objectName: "drop"; target: box; duration: 400 }
        OpacityAnimator { objectName: "thrice"; target: box; from: 0; to: 1; loops: 3; duration: 100 }
        SequentialAnimation {
          objectName: "turn"
          RotationAnimator { target: box; to: 90; duration: 200 }
          ScaleAnimator { target: box; to: 2; duration: 200 }
        }
      }
      """

  Rule: The property is written when the animator ends

    Scenario: While an animator runs, its property keeps the old value
      When the method "start" of "fade" is called
      And 200 milliseconds pass
      Then the property "opacity" of "box" is 1
      And the property "running" of "fade" is true
      And the property "changes" of "root" is 0

    Scenario: When an animator ends, its property is written once
      When the method "start" of "fade" is called
      And 1 second passes
      Then the property "opacity" of "box" is within 0.0001 of 0.2
      And the property "running" of "fade" is false
      And the property "changes" of "root" is 1

    Scenario: An animator with no from starts from the current value
      When the property "opacity" of "box" is set to 0.5
      And the method "start" of "rise" is called
      And 200 milliseconds pass
      Then the property "opacity" of "box" is 0.5
      When 1 second passes
      Then the property "opacity" of "box" is 1

    Scenario: An animator with no to goes to 0
      When the method "start" of "drop" is called
      And 1 second passes
      Then the property "y" of "box" is 0

    Scenario: An animator that loops writes its property only at the very end
      When the property "opacity" of "box" is set to 0.5
      And the property "changes" of "root" is set to 0
      And the method "start" of "thrice" is called
      And 150 milliseconds pass
      Then the property "opacity" of "box" is 0.5
      When 1 second passes
      Then the property "opacity" of "box" is 1
      And the property "changes" of "root" is 1

    Scenario: An animator stopped on the way writes where it got to
      When the method "start" of "fade" is called
      And 200 milliseconds pass
      And the method "stop" of "fade" is called
      Then the property "opacity" of "box" is at most 0.9
      And the property "opacity" of "box" is at least 0.3
      And the property "changes" of "root" is 1

    Scenario: While an animator runs, the picture moves and the property does not
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          width: 400; height: 400; color: "#ffffff"
          Rectangle {
            objectName: "mover"; y: 150; width: 50; height: 50; color: "#000000"
            XAnimator on x { from: 0; to: 300; duration: 4000 }
          }
        }
        """
      When 2 seconds pass
      Then the property "x" of "mover" is 0
      And the pixel at 25, 175 of the window is "#ffffff"
      When 3 seconds pass
      Then the property "x" of "mover" is 300
      And the pixel at 325, 175 of the window is "#000000"

  Rule: Signals

    Scenario: An animator that runs to its end is started, stopped and finished
      When the method "start" of "fade" is called
      Then the property "log" of "root" is "Sr"
      When 1 second passes
      Then the property "log" of "root" is "SrTnF"

    Scenario: An animator stopped on the way finishes before it stops
      When the method "start" of "fade" is called
      And 200 milliseconds pass
      And the method "stop" of "fade" is called
      Then the property "log" of "root" is "SrFTn"

  Rule: Animators in groups and Behaviors

    Scenario: Animators in a SequentialAnimation hold their properties until they end
      When the method "start" of "turn" is called
      And 100 milliseconds pass
      Then the property "rotation" of "box" is 0
      And the property "scale" of "box" is 1
      When 1 second passes
      Then the property "rotation" of "box" is 90
      Then the property "scale" of "box" is 2
      And the property "changes" of "root" is 2

    Scenario: A Behavior with an animator holds the old value until it ends
      When the property "x" of "box" is set to 200
      And 200 milliseconds pass
      Then the property "x" of "box" is 0
      When 1 second passes
      Then the property "x" of "box" is 200
      And the property "changes" of "root" is 1

  Rule: An animator as a value source

    Scenario: An animator on a property starts by itself
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          objectName: "r"; width: 50; height: 50
          ScaleAnimator on scale { from: 0.5; to: 3; duration: 200 }
        }
        """
      When 1 second passes
      Then the property "scale" of "r" is 3

  Rule: An animator in a Transition

    Scenario: A Transition with an animator writes the property when it ends
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          objectName: "r"; width: 50; height: 50
          states: State { name: "gone"; PropertyChanges { target: r; opacity: 0 } }
          transitions: Transition { OpacityAnimator { duration: 400 } }
          id: r
        }
        """
      When the property "state" of "r" is set to "gone"
      And 200 milliseconds pass
      Then the property "opacity" of "r" is 1
      When 1 second passes
      Then the property "opacity" of "r" is 0
