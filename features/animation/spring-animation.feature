# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-springanimation.html
#   https://doc.qt.io/qt-6/qml-qtquick-behavior.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: SpringAnimation
  A SpringAnimation pulls a property towards its value the way a spring would: it speeds up, runs
  past the value and swings back until it rests there. Without a spring it moves at its velocity.

  # Qt runs on real time here, so values on the way are given with a tolerance.

  Rule: A spring pulls the property to its value and lets it swing

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property real pulled: 0
          property real heavy: 0
          property real capped: 0
          property real coarse: 0
          property int whole: 0
          property real peak: 0
          property real turnedAt: 0
          property real rose: peak - turnedAt
          property bool swinging: fine.running
          property bool rocking: rough.running
          onPulledChanged: if (pulled > peak) peak = pulled
          function turnBack() { turnedAt = pulled; peak = pulled; pulled = 20 }
          Behavior on pulled { SpringAnimation { id: fine; spring: 2; damping: 0.2 } }
          Behavior on heavy { SpringAnimation { spring: 2; damping: 0.2; mass: 2 } }
          Behavior on capped { SpringAnimation { spring: 2; damping: 0.2; velocity: 150 } }
          Behavior on coarse { SpringAnimation { id: rough; spring: 2; damping: 0.2; epsilon: 1 } }
          Behavior on whole { SpringAnimation { spring: 2; damping: 0.2 } }
          SpringAnimation { objectName: "plain" }
        }
        """

    Scenario Outline: By default "<property>" is <value>
      Then the property "<property>" of "plain" is <value>

      Examples:
        | property | value |
        | spring   | 0     |
        | damping  | 0     |
        | mass     | 1     |
        | epsilon  | 0.01  |
        | modulus  | 0     |
        | velocity | 0     |

    Scenario: The property is on its way
      When the property "pulled" of "root" is set to 100
      And 100 milliseconds pass
      Then the property "pulled" of "root" is within 12 of 48

    Scenario: It runs past the value
      When the property "pulled" of "root" is set to 100
      And 320 milliseconds pass
      Then the property "pulled" of "root" is within 1.5 of 109.5

    Scenario: It comes to rest at the value
      When the property "pulled" of "root" is set to 100
      And 2 seconds pass
      Then the property "pulled" of "root" is 100
      And the property "swinging" of "root" is false

    Scenario: It is still swinging a little after a second
      When the property "pulled" of "root" is set to 100
      And 1.2 seconds pass
      Then the property "swinging" of "root" is true

    Scenario: A larger epsilon lets it rest sooner
      When the property "coarse" of "root" is set to 100
      And 1.2 seconds pass
      Then the property "coarse" of "root" is 100
      And the property "rocking" of "root" is false

    Scenario: A heavier property is slower to get going
      When the property "heavy" of "root" is set to 100
      And 100 milliseconds pass
      Then the property "heavy" of "root" is within 9 of 28

    Scenario: The velocity is the most it may move at
      When the property "capped" of "root" is set to 100
      And 400 milliseconds pass
      Then the property "capped" of "root" is within 5 of 60

    Scenario: A new value on the way does not take its speed away
      When the property "pulled" of "root" is set to 100
      And 100 milliseconds pass
      And the method "turnBack" of "root" is called
      And 1 second passes
      Then the property "rose" of "root" is within 8 of 13

    Scenario: A whole number comes to rest at the value as well
      When the property "whole" of "root" is set to 100
      And 2 seconds pass
      Then the property "whole" of "root" is 100

  Rule: Without a spring the property moves at the velocity

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property real steady: 0
          property real still: 0
          property bool moving: even.running
          Behavior on steady { SpringAnimation { id: even; velocity: 100 } }
          Behavior on still { SpringAnimation { } }
        }
        """

    Scenario: It moves at the velocity from the start
      When the property "steady" of "root" is set to 100
      And 500 milliseconds pass
      Then the property "steady" of "root" is within 4 of 50

    Scenario: It stops when it is there
      When the property "steady" of "root" is set to 100
      And 1.2 seconds pass
      Then the property "steady" of "root" is 100
      And the property "moving" of "root" is false

    Scenario: Without a spring and without a velocity the property stays where it is
      When the property "still" of "root" is set to 100
      And 500 milliseconds pass
      Then the property "still" of "root" is 0

  Rule: A modulus makes the value wrap around

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property real turn: 350
          property real lap: 350
          Behavior on turn { SpringAnimation { spring: 2; damping: 0.2; modulus: 360 } }
          Behavior on lap { SpringAnimation { velocity: 100; modulus: 360 } }
        }
        """

    Scenario: The property takes the short way through the modulus
      When the property "turn" of "root" is set to 10
      And 300 milliseconds pass
      Then the property "turn" of "root" is within 1 of 12

    Scenario: It comes to rest at the value
      When the property "turn" of "root" is set to 10
      And 2 seconds pass
      Then the property "turn" of "root" is 10

    Scenario: A value past the modulus is wrapped where the spring rests
      When the property "turn" of "root" is set to 370
      And 2 seconds pass
      Then the property "turn" of "root" is 10

    Scenario: At a velocity the property takes the short way as well
      When the property "lap" of "root" is set to 10
      And 150 milliseconds pass
      Then the property "lap" of "root" is within 3 of 5

  Rule: Started by hand it goes from its from value to its to value

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property real free: 10
          property string log
          SpringAnimation {
            objectName: "own"; target: root; property: "free"; from: 50; to: 100
            spring: 2; damping: 0.2
            onStarted: log += "started "; onStopped: log += "stopped "; onFinished: log += "finished "
          }
        }
        """

    Scenario: The way begins at the from value
      When the method "start" of "own" is called
      And 100 milliseconds pass
      Then the property "free" of "root" is within 8 of 72

    Scenario: It arrives at the to value
      When the method "start" of "own" is called
      And 2 seconds pass
      Then the property "free" of "root" is 100
      And the property "running" of "own" is false

    Scenario: Stopped on the way it reports finished as well
      When the method "start" of "own" is called
      And 100 milliseconds pass
      And the method "stop" of "own" is called
      Then the property "log" of "root" is "started finished stopped "
