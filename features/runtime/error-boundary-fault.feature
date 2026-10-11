# Sources:
#   docs/ARCHITECTURE.md §9.3 (faults)
#   docs/ARCHITECTURE.md §9.4 (Supervisor: UI boundaries, ErrorBoundary)

@ljos-only
Feature: ErrorBoundary says what went wrong
  The boundary's `fault` has the kind of the fault, its message and the
  QML file and line it belongs to, so the fallback can show them.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import Ljos
      ErrorBoundary {
        id: boundary
        objectName: "boundary"
        catchExceptions: true
        property bool inThisFile: fault.file.toString().endsWith("Scenario.qml")
        fallback: Text { objectName: "sorry"; text: boundary.fault.kind + ": " + boundary.fault.message }
        Item {
          objectName: "content"
          property int n: 0
          onNChanged: { throw new TypeError("not a page") }
          function spin() { while (true) { } }
        }
      }
      """

  Scenario: There is no fault while the content runs
    Then the property "fault.kind" of "boundary" is ""
    And the property "fault.message" of "boundary" is ""
    And the property "fault.line" of "boundary" is 0

  Scenario: A panic is a fault of the kind Panic
    When a native handler of "content" panics with "boom"
    Then the property "fault.kind" of "boundary" is "Panic"
    And the property "fault.message" of "boundary" is "boom"

  Scenario: A panic belongs to the line its object is written on
    When a native handler of "content" panics with "boom"
    Then the property "inThisFile" of "boundary" is true
    And the property "fault.line" of "boundary" is 9

  Scenario: An exception belongs to the line that threw
    When the property "n" of "content" is set to 1
    Then the property "fault.kind" of "boundary" is "JsException"
    And the property "fault.message" of "boundary" is "TypeError: not a page"
    And the property "fault.line" of "boundary" is 12

  Scenario: A handler that never returns is a fault of the kind JsInterrupted
    When the method "spin" of "content" is called
    Then the property "fault.kind" of "boundary" is "JsInterrupted"
    And the property "failed" of "boundary" is true

  Scenario: The fallback can show the fault
    When a native handler of "content" panics with "boom"
    Then the property "text" of "sorry" is "Panic: boom"
