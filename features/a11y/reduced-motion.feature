# Sources:
#   https://doc.qt.io/qt-6/qaccessibilityhints.html#motionPreference-prop
#   https://doc.qt.io/qt-6/qml-qtquick-animation.html
#   https://doc.qt.io/qt-6/qml-qtquick-behavior.html
#   https://www.w3.org/WAI/WCAG22/Understanding/animation-from-interactions.html
#   https://developer.apple.com/design/human-interface-guidelines/motion
#   https://m3.material.io/foundations/designing/motion
#   https://web.dev/articles/prefers-reduced-motion
#   docs/ARCHITECTURE.md (section 14, SystemPrefs)
#   docs/design/motion.md (reduced motion)
# Qt tells an application that the user wants less motion and leaves the rest
# to it. Ours: what moves an item (a slide, a zoom, a turn) is at its end at
# once, what only fades or changes colour still does so, at most 150
# milliseconds long, and what runs for ever goes on running, since it says
# that something is still busy. Runs on Ljós only.

@ljos-only @a11y
Feature: Reduced motion
  When the system asks for reduced motion nothing slides, grows or turns on
  its way to a value: it is there at once. A fade or a change of colour is
  not movement and still shows that something changed, only quicker. An
  animation that runs for ever keeps running. The signals are the same.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 300; height: 200
        property string log: ""; property int ticks: 0
        Rectangle {
          id: box; objectName: "box"; width: 20; height: 20; color: "#ff0000"
          Behavior on y { NumberAnimation { duration: 1000 } } Behavior on color { ColorAnimation { duration: 1000 } }
        }
        NumberAnimation {
          objectName: "slide"; target: box; property: "x"; from: 0; to: 100; duration: 1000
          onStarted: root.log += "started "; onFinished: root.log += "finished "
        }
        NumberAnimation {
          objectName: "fade"; target: box; property: "opacity"; from: 1; to: 0; duration: 1000
          onStarted: root.log += "started "; onFinished: root.log += "finished "
        }
        NumberAnimation { objectName: "blink"; target: box; property: "opacity"; from: 1; to: 0; duration: 100 }
        SequentialAnimation {
          objectName: "pulse"; loops: Animation.Infinite
          NumberAnimation { target: box; property: "opacity"; from: 1; to: 0.2; duration: 400 }
          NumberAnimation { target: box; property: "opacity"; from: 0.2; to: 1; duration: 400 }
        }
        RotationAnimation { objectName: "spin"; target: box; property: "rotation"; from: 0; to: 360; duration: 1200; loops: Animation.Infinite }
        Timer { objectName: "timer"; interval: 500; repeat: true; onTriggered: root.ticks++ }
      }
      """

  Rule: With reduced motion

    Background:
      Given the system setting "reduced motion" is true

    Scenario: A slide is at its end in the step it starts in
      When the method "start" of "slide" is called
      And 16 milliseconds pass
      Then the property "x" of "box" is 100
      And the property "running" of "slide" is false

    Scenario: A slide still says that it started and finished, in that order
      When the method "start" of "slide" is called
      And 16 milliseconds pass
      Then the property "log" of "root" is "started finished "

    Scenario: A Behavior that moves takes the property to its new value at once
      When the property "y" of "box" is set to 80
      And 16 milliseconds pass
      Then the property "y" of "box" is 80

    Scenario: A long fade still fades, in at most 150 milliseconds
      When the method "start" of "fade" is called
      And 75 milliseconds pass
      Then the property "opacity" of "box" is within 0.15 of 0.5
      And the property "running" of "fade" is true
      When 100 milliseconds pass
      Then the property "opacity" of "box" is 0
      And the property "running" of "fade" is false

    Scenario: A fade still says that it started and finished, in that order
      When the method "start" of "fade" is called
      And 16 milliseconds pass
      Then the property "log" of "root" is "started "
      When 200 milliseconds pass
      Then the property "log" of "root" is "started finished "

    Scenario: A fade shorter than that keeps its own length
      When the method "start" of "blink" is called
      And 50 milliseconds pass
      Then the property "opacity" of "box" is within 0.15 of 0.5

    Scenario: A change of colour still shows on its way
      When the property "color" of "box" is set to "#0000ff"
      And 75 milliseconds pass
      Then the property "color" of "box" is not "#ff0000"
      And the property "color" of "box" is not "#0000ff"
      When 100 milliseconds pass
      Then the property "color" of "box" is "#0000ff"

    Scenario: An endless turn keeps turning at its own pace
      When the method "start" of "spin" is called
      And 300 milliseconds pass
      Then the property "rotation" of "box" is within 5 of 90
      When 300 milliseconds pass
      Then the property "rotation" of "box" is within 5 of 180
      And the property "running" of "spin" is true

    Scenario: An endless pulse keeps pulsing at its own pace
      When the method "start" of "pulse" is called
      And 200 milliseconds pass
      Then the property "opacity" of "box" is within 0.1 of 0.6
      When 200 milliseconds pass
      Then the property "opacity" of "box" is at most 0.3
      When 400 milliseconds pass
      Then the property "opacity" of "box" is at least 0.9

    Scenario: An endless animation can still be stopped
      When the method "start" of "spin" is called
      And 16 milliseconds pass
      And the method "stop" of "spin" is called
      Then the property "running" of "spin" is false

    Scenario: A timer is not motion and keeps its interval
      When the method "start" of "timer" is called
      And 1 second passes
      Then the property "ticks" of "root" is 2

  Rule: The setting changes while the application runs

    Scenario: Without reduced motion an animation takes its time
      When the method "start" of "slide" is called
      And 500 milliseconds pass
      Then the property "x" of "box" is within 5 of 50

    Scenario: A slide that runs when motion is reduced ends at once
      When the method "start" of "slide" is called
      And 200 milliseconds pass
      And the system setting "reduced motion" changes to true
      And 16 milliseconds pass
      Then the property "x" of "box" is 100
      And the property "running" of "slide" is false
      And the property "log" of "root" is "started finished "

    Scenario: A fade that runs when motion is reduced goes on from where it is, quicker
      When the method "start" of "fade" is called
      And 500 milliseconds pass
      And the system setting "reduced motion" changes to true
      And 16 milliseconds pass
      Then the property "opacity" of "box" is at most 0.5
      And the property "opacity" of "box" is at least 0.3
      And the property "running" of "fade" is true
      When 150 milliseconds pass
      Then the property "opacity" of "box" is 0
      And the property "running" of "fade" is false

    Scenario: An endless turn that runs when motion is reduced keeps turning
      When the method "start" of "spin" is called
      And 300 milliseconds pass
      And the system setting "reduced motion" changes to true
      And 300 milliseconds pass
      Then the property "rotation" of "box" is within 5 of 180

    Scenario: A fade takes its own time again from where it is when motion is no longer reduced
      When the system setting "reduced motion" changes to true
      And the method "start" of "fade" is called
      And 50 milliseconds pass
      And the system setting "reduced motion" changes to false
      And 16 milliseconds pass
      Then the property "opacity" of "box" is at least 0.5
      When 300 milliseconds pass
      Then the property "running" of "fade" is true
      When 1 second passes
      Then the property "opacity" of "box" is 0
      And the property "running" of "fade" is false

    Scenario: A slide takes its time again when motion is no longer reduced
      When the system setting "reduced motion" changes to true
      And the system setting "reduced motion" changes to false
      And the method "start" of "slide" is called
      And 500 milliseconds pass
      Then the property "x" of "box" is within 5 of 50
