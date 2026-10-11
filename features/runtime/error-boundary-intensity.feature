# Sources:
#   docs/ARCHITECTURE.md §9.4 (Supervisor: restart intensity, escalation)

@ljos-only
Feature: ErrorBoundary gives up when its content keeps failing
  A boundary allows `maxRestarts` restarts within `within` milliseconds.
  One fault more in that time and it gives up: its content stays
  destroyed and the fault becomes a fault of the boundary around it.
  With no boundary around it, it becomes the application's fault.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import Ljos
      ErrorBoundary {
        objectName: "outer"
        fallback: Item { objectName: "outerSorry" }
        Item {
          objectName: "page"
          ErrorBoundary {
            objectName: "inner"
            maxRestarts: 2
            within: 1000
            Item { objectName: "content" }
          }
        }
      }
      """

  Scenario: Faults within the limit are restarted
    When a native handler of "content" panics with "one"
    And a native handler of "content" panics with "two"
    Then the object "inner/content" exists
    And the property "restarts" of "inner" is 2
    And the property "failed" of "outer" is false

  Scenario: One fault too many becomes the outer boundary's fault
    When a native handler of "content" panics with "one"
    And a native handler of "content" panics with "two"
    And a native handler of "content" panics with "three"
    Then the object "inner" does not exist
    And the object "outer/outerSorry" exists
    And the property "fault.message" of "outer" is "three"

  Scenario: Faults far apart do not add up
    When a native handler of "content" panics with "one"
    And a native handler of "content" panics with "two"
    And 2 seconds pass
    And a native handler of "content" panics with "three"
    Then the object "inner/content" exists
    And the property "restarts" of "inner" is 3
    And the property "failed" of "outer" is false

  Scenario: The outer boundary's own content can be made again
    When a native handler of "content" panics with "one"
    And a native handler of "content" panics with "two"
    And a native handler of "content" panics with "three"
    And the method "retry" of "outer" is called
    Then the object "inner/content" exists
    And the property "restarts" of "inner" is 0
