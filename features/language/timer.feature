# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-timer.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Timer
  A Timer triggers once, or every interval while it runs, as application time passes.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property int ticks: 0
        property int shots: 0
        Timer {
          id: repeating
          objectName: "repeating"
          interval: 100; repeat: true
          onTriggered: ticks++
        }
        Timer { objectName: "once"; interval: 50; running: true; onTriggered: shots++ }
        function start() { repeating.start() }
        function stop() { repeating.stop() }
      }
      """

  Scenario: A timer does not trigger before it is started
    When 1 second passes
    Then the property "ticks" of "root" is 0

  Scenario: A repeating timer triggers every interval
    When the method "start" of "root" is called
    And 350 milliseconds pass
    Then the property "ticks" of "root" is 3

  Scenario: A stopped timer triggers no more
    When the method "start" of "root" is called
    And 150 milliseconds pass
    And the method "stop" of "root" is called
    And 500 milliseconds pass
    Then the property "ticks" of "root" is 1

  Scenario: A single-shot timer triggers once and stops running
    When 500 milliseconds pass
    Then the property "shots" of "root" is 1
    And the property "running" of "once" is false

  Scenario: triggeredOnStart triggers as the timer starts
    When the property "triggeredOnStart" of "repeating" is set to true
    And the method "start" of "root" is called
    And 10 milliseconds pass
    Then the property "ticks" of "root" is 1
