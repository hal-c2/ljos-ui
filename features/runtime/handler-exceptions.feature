# Sources:
#   https://doc.qt.io/qt-6/qtqml-syntax-signals.html
#   https://doc.qt.io/qt-6/qtquick-debugging.html
#   docs/ARCHITECTURE.md §9.3 (faults)
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: An exception in a handler stops only that handler
  A handler that throws is cut short and the exception is logged. The
  other handlers of the signal, the code that emitted it and everything
  else in the application carry on.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property string events: ""
        property int count: 0
        signal poke()
        onPoke: { events += "first "; throw new Error("first failed") }
        Connections { target: root; function onPoke() { root.events += "second "; throw new Error("second failed") } }
        onCountChanged: { events += "count "; null.x = 1 }
        Item { property int twice: root.count * 2; onTwiceChanged: root.events += "twice" + twice + " " }
        function emitIt() { poke(); events += "after " }
        function thrower() { throw "a string" }
        function guarded() { try { thrower() } catch (e) { events += "caught:" + e } }
      }
      """

  Scenario: The other handlers of the signal still run
    When the method "emitIt" of "root" is called
    Then the property "events" of "root" is "first second after "

  Scenario: A handler of a property change that throws does not stop the others
    When the property "count" of "root" is set to 3
    Then the property "events" of "root" is "count twice6 "
    And the property "count" of "root" is 3

  Scenario: The application still works afterwards
    When the method "emitIt" of "root" is called
    And the method "emitIt" of "root" is called
    Then the property "events" of "root" is "first second after first second after "

  Scenario: Each exception is logged with its file and line
    When the method "emitIt" of "root" is called
    Then the log has a warning matching "Scenario.qml:8: .*first failed"
    And the log has a warning matching "Scenario.qml:9: .*second failed"

  Scenario: An exception that the JavaScript catches is not logged
    When the method "guarded" of "root" is called
    Then the property "events" of "root" is "caught:a string"
    And the log has no warnings
