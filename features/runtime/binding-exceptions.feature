# Sources:
#   https://doc.qt.io/qt-6/qtqml-syntax-propertybinding.html
#   https://doc.qt.io/qt-6/qtquick-debugging.html
#   docs/ARCHITECTURE.md §9.3 (faults)
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: An exception in a binding stops only that binding
  A binding that throws leaves its property as it was and is logged with
  the line the exception was thrown on. The other bindings of the object
  keep working.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property int n: 2
        property int bad: { throw new Error("no value") }
        property int deep: helper()
        property int good: n * 2
        function helper() { return inner() }
        function inner() {
          throw new Error("too deep")
        }
      }
      """

  Scenario: The other bindings have their values
    Then the property "good" of "root" is 4
    And the property "bad" of "root" is 0
    And the property "deep" of "root" is 0

  Scenario: The other bindings still follow what they read
    When the property "n" of "root" is set to 5
    Then the property "good" of "root" is 10

  Scenario: The exception is logged with the binding's file and line
    Then the log has a warning matching "Scenario.qml:6: .*no value"

  Scenario: An exception thrown in a function is logged with the line that threw
    Then the log has a warning matching "Scenario.qml:11: .*too deep"
