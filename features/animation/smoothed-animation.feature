# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-smoothedanimation.html
#   https://doc.qt.io/qt-6/qml-qtquick-behavior.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: SmoothedAnimation
  A SmoothedAnimation takes a property to its value at a velocity, speeding up at the start and
  slowing down at the end. A duration only makes it faster, never slower.

  # Qt runs on real time here, so values on the way are given with a tolerance.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property real level: 0
        property real linear: 0
        property real brief: 0
        property real capped: 0
        property real fast: 0
        property real stuck: 0
        property real jump: 0
        property real snap: 0
        Behavior on level { SmoothedAnimation { velocity: 100 } }
        Behavior on linear { SmoothedAnimation { velocity: 100; maximumEasingTime: 0 } }
        Behavior on brief { SmoothedAnimation { velocity: 100; maximumEasingTime: 200 } }
        Behavior on capped { SmoothedAnimation { velocity: 10; duration: 400 } }
        Behavior on fast { SmoothedAnimation { velocity: 1000; duration: 5000 } }
        Behavior on stuck { SmoothedAnimation { velocity: 0; duration: 100 } }
        Behavior on jump { SmoothedAnimation { velocity: -1 } }
        Behavior on snap { SmoothedAnimation { duration: 0 } }
        SmoothedAnimation { objectName: "plain" }
      }
      """

  Scenario: The defaults are a velocity of 200 and no duration
    Then the property "velocity" of "plain" is 200
    And the property "duration" of "plain" is -1
    And the property "maximumEasingTime" of "plain" is -1
    And the property "reversingMode" of "plain" is 0

  Scenario: The distance and the velocity give the time it takes
    When the property "level" of "root" is set to 100
    And 1.5 seconds pass
    Then the property "level" of "root" is 100

  Scenario: It speeds up at the start
    When the property "level" of "root" is set to 100
    And 250 milliseconds pass
    Then the property "level" of "root" is within 4 of 12.5

  Scenario: It is half way after half the time
    When the property "level" of "root" is set to 100
    And 500 milliseconds pass
    Then the property "level" of "root" is within 8 of 50

  Scenario: Without easing time it moves at the velocity from the start
    When the property "linear" of "root" is set to 100
    And 250 milliseconds pass
    Then the property "linear" of "root" is within 4 of 25

  Scenario: A short easing time leaves most of the way at one speed
    When the property "brief" of "root" is set to 100
    And 250 milliseconds pass
    Then the property "brief" of "root" is within 4 of 22.2

  Scenario: A duration shorter than the velocity allows is kept
    When the property "capped" of "root" is set to 100
    And 1 second passes
    Then the property "capped" of "root" is 100

  Scenario: A duration longer than the velocity needs is not waited for
    When the property "fast" of "root" is set to 100
    And 500 milliseconds pass
    Then the property "fast" of "root" is 100

  Scenario: At a velocity of zero the property stays where it is
    When the property "stuck" of "root" is set to 100
    And 1 second passes
    Then the property "stuck" of "root" is 0

  Scenario: Without a velocity and without a duration the value is taken at once
    When the property "jump" of "root" is set to 100
    Then the property "jump" of "root" is 100

  Scenario: With a duration of zero the value is taken at once
    When the property "snap" of "root" is set to 100
    Then the property "snap" of "root" is 100
