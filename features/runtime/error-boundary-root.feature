# Sources:
#   docs/ARCHITECTURE.md §9.4 (Supervisor: escalation; the application is the last boundary)

@ljos-only
Feature: A boundary that gives up with nothing around it tells the application
  The application is the boundary of everything that has no other. A
  fault that reaches it destroys nothing and is handed to the host, which
  can show it.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import Ljos
      Item {
        objectName: "root"
        ErrorBoundary {
          objectName: "boundary"
          maxRestarts: 1
          Item { objectName: "content" }
        }
      }
      """

  Scenario: A fault a boundary handles is not the application's
    When a native handler of "content" panics with "one"
    Then the object "boundary/content" exists
    And the application has no fault

  Scenario: A boundary that gives up hands the fault to the application
    When a native handler of "content" panics with "one"
    And a native handler of "content" panics with "two"
    Then the application fault is a "Panic" matching "two"
    And the property "failed" of "boundary" is true
    And the object "content" does not exist

  Scenario: The rest of the application lives on
    When a native handler of "content" panics with "one"
    And a native handler of "content" panics with "two"
    Then the object "boundary" exists
    And the object "root" exists
