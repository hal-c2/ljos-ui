# Sources:
#   docs/ARCHITECTURE.md sections 3.4, 9.3, 9.4 and 19.4
#   https://www.erlang.org/doc/apps/observer/observer_ug.html (Applications)
# The inspector is a Ljós extension: Qt has nothing to compare it with.

@ljos-only @headless
Feature: The supervision tree and the faults from the inspector
  Erlang's observer shows an application's supervision tree: which
  supervisor restarts what, and how often it may. An agent or the
  `ljos observe` window sees a Ljós application's tree the same way: the
  boundaries that have had a fault, each under the one around it, with
  the restarts it allows within how long and the ones it has used. With
  them come the last faults of every kind, each with where it happened
  and what took it: a boundary, the application, or only the log.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import Ljos
      Item {
        objectName: "top"
        function oops() { throw new Error("not here") }
        ErrorBoundary {
          objectName: "outer"
          fallback: Item { objectName: "outerSorry" }
          Item {
            ErrorBoundary {
              objectName: "inner"
              maxRestarts: 2
              within: 1000
              Item { objectName: "content" }
            }
          }
        }
      }
      """
    And the inspector is connected

  Rule: The tree shows each supervisor, what it allows and what it used

    Scenario: An application that never faulted has the standard supervisors
      When the agent calls the tool "app_processes" with:
        """json
        {"kind": "supervisor"}
        """
      Then the answer's "/processes" has 4 entries
      And the answer's "/processes/0/name" is "app"
      And the answer's "/processes/0/depth" is 0
      And the answer's "/processes/1/name" is "ui"
      And the answer's "/processes/1/depth" is 1
      And the answer's "/processes/1/strategy" is "one_for_one"

    Scenario: An application that never faulted lists no faults
      When the agent calls the tool "app_processes" with:
        """json
        {"kind": "fault"}
        """
      Then the answer's "/faults" has 0 entries

    Scenario: A boundary that had a fault is a supervisor under the one around it
      When a native handler of "content" panics with "one"
      And the agent calls the tool "app_processes" with:
        """json
        {"kind": "supervisor"}
        """
      Then the answer's "/processes" has 6 entries
      And the answer's "/processes/2/name" is "ErrorBoundary \"outer\""
      And the answer's "/processes/2/depth" is 2
      And the answer's "/processes/2/restarts" is 0
      And the answer's "/processes/3/name" is "ErrorBoundary \"inner\""
      And the answer's "/processes/3/locator" is "#inner"
      And the answer's "/processes/3/depth" is 3
      And the answer's "/processes/3/max_restarts" is 2
      And the answer's "/processes/3/within_ms" is 1000
      And the answer's "/processes/3/restarts" is 1
      And the answer's "/processes/3/content" is "running"

    Scenario: A boundary that gave up is gone with the content around it
      When a native handler of "content" panics with "one"
      And a native handler of "content" panics with "two"
      And a native handler of "content" panics with "three"
      And the agent calls the tool "app_processes" with:
        """json
        {"kind": "supervisor"}
        """
      Then the answer's "/processes" has 5 entries
      And the answer's "/processes/2/name" is "ErrorBoundary \"outer\""
      And the answer's "/processes/2/content" is "running"

  Rule: Each fault says where it happened and what took it

    Scenario: A fault inside a boundary is the boundary's
      When a native handler of "content" panics with "one"
      And the agent calls the tool "app_processes" with:
        """json
        {"kind": "fault"}
        """
      Then the answer's "/faults" has 1 entries
      And the answer's "/faults/0/fault" is "Panic"
      And the answer's "/faults/0/message" is "one"
      And the answer's "/faults/0/origin" is "Item \"content\""
      And the answer's "/faults/0/origin_gone" is true
      And the answer's "/faults/0/taken_by" is "ErrorBoundary \"inner\""
      And the answer's "/processes" has 0 entries

    Scenario: The fault a boundary gave up on is the next one's
      When a native handler of "content" panics with "one"
      And a native handler of "content" panics with "two"
      And a native handler of "content" panics with "three"
      And the agent calls the tool "app_processes" with:
        """json
        {"kind": "fault"}
        """
      Then the answer's "/faults" has 3 entries
      And the answer's "/faults/1/taken_by" is "ErrorBoundary \"inner\""
      And the answer's "/faults/2/taken_by" is "ErrorBoundary \"outer\""
      And the answer's "/faults/2/origin" is "Item \"content\""

    Scenario: A JS exception outside every boundary is only logged
      When the method "oops" of "top" is called
      And the agent calls the tool "app_processes" with:
        """json
        {"kind": "fault"}
        """
      Then the answer's "/faults" has 1 entries
      And the answer's "/faults/0/fault" is "JsException"
      And the answer's "/faults/0/message" matches "not here"
      And the answer's "/faults/0/taken_by" is "log"
      And the answer's "/faults/0/line" is 5

    Scenario: A fault outside every boundary is the application's
      When a native handler of "top" panics with "bare"
      And the agent calls the tool "app_processes" with:
        """json
        {"kind": "fault"}
        """
      Then the answer's "/faults/0/taken_by" is "application"
