# Sources:
#   docs/ARCHITECTURE.md §9.3 (faults: an exception does not stop other handlers)
#   docs/ARCHITECTURE.md §9.4 (Supervisor: UI boundaries, ErrorBoundary)

@ljos-only
Feature: ErrorBoundary leaves exceptions alone unless asked
  An exception in a handler or a binding is logged and the application
  carries on, as in Qt, so by default it is not a fault of a boundary.
  A boundary with `catchExceptions` treats one as it treats a panic.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import Ljos
      Item {
        objectName: "root"
        ErrorBoundary {
          objectName: "lenient"
          fallback: Item { objectName: "lenientSorry" }
          Item { objectName: "one"; property int n: 0; onNChanged: { throw new Error("one failed") } }
        }
        ErrorBoundary {
          objectName: "strict"
          catchExceptions: true
          fallback: Item { objectName: "strictSorry" }
          Item { objectName: "two"; property int n: 0; onNChanged: { throw new Error("two failed") } }
        }
      }
      """

  Scenario: By default an exception is only logged
    When the property "n" of "one" is set to 1
    Then the object "lenient/one" exists
    And the property "failed" of "lenient" is false
    And the log has a warning matching "Scenario.qml:8: .*one failed"

  Scenario: With catchExceptions an exception is a fault
    When the property "n" of "two" is set to 1
    Then the object "two" does not exist
    And the object "strict/strictSorry" exists
    And the property "failed" of "strict" is true

  Scenario: The exception is logged all the same
    When the property "n" of "two" is set to 1
    Then the log has a warning matching "Scenario.qml:14: .*two failed"

  Scenario: One boundary's fault is not another's
    When the property "n" of "two" is set to 1
    Then the object "lenient/one" exists
    And the property "failed" of "lenient" is false
