# Sources:
#   https://doc.qt.io/qt-6/qtquick-debugging.html
#   docs/ARCHITECTURE.md §9.3 (faults: "logged with its QML file, line and stack")

# Qt logs the line an exception was thrown on and nothing of how the
# program got there. Ljós adds the functions it passed through.
@ljos-only
Feature: A logged exception says how the program got there
  An exception thrown in a function that was called from another names
  both, each with its QML file and line, under the line Qt would log.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property int value: 0
        function outer() { return inner() }
        function inner() {
          throw new Error("too deep")
        }
        function direct() { throw new Error("at once") }
        function run() { value = outer() }
      }
      """

  Scenario: The function that threw comes first
    When the method "run" of "root" is called
    Then the log has a warning matching "Scenario.qml:8: .*too deep"
    And the log has a warning matching "at inner \(Scenario.qml:8\)"

  Scenario: The function that called it follows
    When the method "run" of "root" is called
    Then the log has a warning matching "at inner \(Scenario.qml:8\)\n\s+at outer \(Scenario.qml:6\)"

  Scenario: An exception thrown where it is logged has one line
    When the method "direct" of "root" is called
    Then the log has a warning matching "Scenario.qml:10: .*at once"
    And the log has no warning matching "at direct"
