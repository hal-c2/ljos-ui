# Sources:
#   https://doc.qt.io/qt-6/qtqml-syntax-signals.html
#   https://doc.qt.io/qt-6/qtquick-debugging.html
# Verified on Qt 6.11.2 (qml6, offscreen).

# Waits for the compiler: a name that nothing declares is compiled to a
# run-time lookup that gives `undefined` (`Arg::Dynamic` in `ljos_ir`), so
# reading it does not throw and calling it is a TypeError. Qt throws a
# ReferenceError in both cases.
@qt-parity @backlog
Feature: A name that nothing declares is a ReferenceError
  Reading or calling a name that no scope has throws, like any other
  exception: the handler or binding is cut short, the exception is logged
  with its file and line, and the rest of the application carries on.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property string events: ""
        property int n: 0
        property int bad: nothing.value
        property int good: n * 2
        onNChanged: { events += "before "; missing(); events += "after " }
      }
      """

  Scenario: Reading the name in a binding is logged
    Then the log has a warning matching "Scenario.qml:7: ReferenceError: nothing is not defined"
    And the property "bad" of "root" is 0

  Scenario: Calling the name in a handler is logged
    When the property "n" of "root" is set to 1
    Then the log has a warning matching "Scenario.qml:9: ReferenceError: missing is not defined"

  Scenario: The handler stops at the name
    When the property "n" of "root" is set to 1
    Then the property "events" of "root" is "before "
    And the property "good" of "root" is 2
