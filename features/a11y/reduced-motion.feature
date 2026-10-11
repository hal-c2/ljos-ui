# Sources:
#   https://doc.qt.io/qt-6/qaccessibilityhints.html#motionPreference-prop
#   https://doc.qt.io/qt-6/qml-qtquick-animation.html
#   https://doc.qt.io/qt-6/qml-qtquick-behavior.html
#   docs/ARCHITECTURE.md (section 14, SystemPrefs)
# Qt tells an application that the user wants less motion and leaves the rest
# to it. Ours: every animation then ends in the step it starts in, so that an
# application is calm without a line of its own. Runs on Ljós only.

@ljos-only @a11y
Feature: Reduced motion
  When the system asks for reduced motion nothing slides or fades: a property
  that would be animated is at its end value at once.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        width: 300; height: 200
        property string log: ""
        property int ticks: 0
        Rectangle {
          id: box; objectName: "box"; width: 20; height: 20
          Behavior on y { NumberAnimation { duration: 1000 } }
        }
        NumberAnimation {
          objectName: "slide"; target: box; property: "x"; from: 0; to: 100; duration: 1000
          onStarted: root.log += "started "; onFinished: root.log += "finished "
        }
        SequentialAnimation {
          objectName: "pulse"; loops: Animation.Infinite
          NumberAnimation { target: box; property: "opacity"; from: 1; to: 0.2; duration: 400 }
          NumberAnimation { target: box; property: "opacity"; from: 0.2; to: 1; duration: 400 }
        }
        Timer { objectName: "timer"; interval: 500; repeat: true; onTriggered: root.ticks++ }
      }
      """

  Rule: With reduced motion

    Background:
      Given the system setting "reduced motion" is true

    Scenario: An animation is at its end in the step it starts in
      When the method "start" of "slide" is called
      And 16 milliseconds pass
      Then the property "x" of "box" is 100
      And the property "running" of "slide" is false

    Scenario: An animation still says that it started and finished
      When the method "start" of "slide" is called
      And 16 milliseconds pass
      Then the property "log" of "root" is "started finished "

    Scenario: A Behavior takes the property to its new value at once
      When the property "y" of "box" is set to 80
      And 16 milliseconds pass
      Then the property "y" of "box" is 80

    Scenario: An endless animation rests at the end of its round
      When the method "start" of "pulse" is called
      And 16 milliseconds pass
      Then the property "opacity" of "box" is 1
      And the property "running" of "pulse" is true

    Scenario: An endless animation stays at rest
      When the method "start" of "pulse" is called
      And 3 seconds pass
      Then the property "opacity" of "box" is 1

    Scenario: An endless animation can still be stopped
      When the method "start" of "pulse" is called
      And 16 milliseconds pass
      And the method "stop" of "pulse" is called
      Then the property "running" of "pulse" is false

    Scenario: A timer is not motion and keeps its interval
      When the method "start" of "timer" is called
      And 1 second passes
      Then the property "ticks" of "root" is 2

  Rule: The setting changes while the application runs

    Scenario: Without reduced motion an animation takes its time
      When the method "start" of "slide" is called
      And 500 milliseconds pass
      Then the property "x" of "box" is within 5 of 50

    Scenario: An animation that runs when motion is reduced ends at once
      When the method "start" of "slide" is called
      And 200 milliseconds pass
      And the system setting "reduced motion" changes to true
      And 16 milliseconds pass
      Then the property "x" of "box" is 100
      And the property "running" of "slide" is false

    Scenario: An endless animation that runs when motion is reduced comes to rest
      When the method "start" of "pulse" is called
      And 200 milliseconds pass
      And the system setting "reduced motion" changes to true
      And 16 milliseconds pass
      Then the property "opacity" of "box" is 1

    Scenario: An animation takes its time again when motion is no longer reduced
      When the system setting "reduced motion" changes to true
      And the system setting "reduced motion" changes to false
      And the method "start" of "slide" is called
      And 500 milliseconds pass
      Then the property "x" of "box" is within 5 of 50

    Scenario: An endless animation moves again when motion is no longer reduced
      When the system setting "reduced motion" changes to true
      And the method "start" of "pulse" is called
      And 16 milliseconds pass
      And the system setting "reduced motion" changes to false
      And 200 milliseconds pass
      Then the property "opacity" of "box" is at most 0.9
