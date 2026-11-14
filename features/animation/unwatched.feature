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
  An animation of a property that nothing binds to, listens to or draws asks
  for no frames. Its value is worked out when it is read, so a script sees
  the value of that moment. Something that starts to watch the property
  catches the animation up and keeps it going frame by frame, and an
  animation with an end still finishes on time.

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

  Scenario: A change handler hears an animation frame by frame
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
