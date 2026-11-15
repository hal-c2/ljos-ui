# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-numberanimation.html
#   https://doc.qt.io/qt-6/qml-qtquick-animation.html#finished-signal
# What a property nobody binds to, listens to or draws holds between two
# reads cannot be seen, so how often it is worked out while nobody looks is
# Ljós's own (0 % CPU when idle). What can be seen is checked against qml6:
# the value when read, a binding that comes later, a change handler, and
# `finished` on time.

@ljos-only
Feature: An animation nobody watches
  An animation of a property that nothing binds to, listens to or draws
  schedules nothing. Its value is worked out when it is read, so a script
  sees the value of that moment. Something that starts to watch the property
  catches the animation up and keeps it going, a value given to the property
  lasts only until the animation's next step, and an animation with an end
  still finishes on time, sooner when the system asks for reduced motion.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string log
        property real seen: -1
        property real looked: -1
        property int heard: 0
        NumberAnimation {
          id: spin; objectName: "spin"; target: spin; property: "phase"
          property real phase: 0
          from: 0; to: 360; duration: 1000; loops: Animation.Infinite
        }
        NumberAnimation {
          id: once; objectName: "once"; target: once; property: "phase"
          property real phase: 0
          from: 0; to: 100; duration: 100
          onFinished: root.log += "finished "
        }
        NumberAnimation {
          id: slow; objectName: "slow"; target: slow; property: "phase"
          property real phase: 0
          from: 0; to: 100; duration: 1000
          onFinished: root.log += "slow finished "
        }
        function look() { looked = spin.phase }
        function follow() { seen = Qt.binding(function() { return spin.phase }) }
        function listen() { spin.phaseChanged.connect(function() { heard++ }) }
      }
      """

  Scenario: An endless animation nobody watches schedules nothing
    When the method "start" of "spin" is called
    And 100 milliseconds pass
    Then the application has nothing scheduled
    And the property "running" of "spin" is true

  Scenario: A script that reads an animation nobody watches sees the value of its time
    When the method "start" of "spin" is called
    And 250 milliseconds pass
    And the method "look" of "root" is called
    Then the property "looked" of "root" is within 1 of 90
    And the application has nothing scheduled

  Scenario: A binding that comes to an animation nobody watches catches it up and follows it
    When the method "start" of "spin" is called
    And 250 milliseconds pass
    And the method "follow" of "root" is called
    Then the property "seen" of "root" is within 1 of 90
    When 250 milliseconds pass
    Then the property "seen" of "root" is within 1 of 180

  Scenario: A change handler hears every change of an animation
    When the method "start" of "spin" is called
    And 100 milliseconds pass
    And the method "listen" of "root" is called
    And 100 milliseconds pass
    Then the property "heard" of "root" is at least 5

  Scenario: An animation nobody watches with an end finishes on time
    When the method "start" of "once" is called
    And 99 milliseconds pass
    Then the property "log" of "root" is ""
    When 1 millisecond passes
    Then the property "log" of "root" is "finished "
    And the property "running" of "once" is false
    And the property "phase" of "once" is 100
    And the application has nothing scheduled

  Scenario: A value given to an animation nobody watches lasts until its next step
    When the method "start" of "spin" is called
    And 250 milliseconds pass
    And the property "phase" of "spin" is set to 5
    And 250 milliseconds pass
    Then the property "phase" of "spin" is within 1 of 180
    And the application has nothing scheduled

  Scenario: An animation nobody watches ends at once when the system asks for reduced motion
    When the method "start" of "slow" is called
    And 200 milliseconds pass
    And the system setting "reduced motion" changes to true
    And 16 milliseconds pass
    Then the property "log" of "root" is "slow finished "
    And the property "running" of "slow" is false
    And the property "phase" of "slow" is 100
