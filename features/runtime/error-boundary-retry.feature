# Sources:
#   docs/ARCHITECTURE.md §9.4 (Supervisor: UI boundaries restart from their component)

@ljos-only
Feature: ErrorBoundary.retry makes the content again
  While the fallback shows, `retry()` destroys it and makes the content
  again from its definition. A boundary without a fallback does that by
  itself as soon as the fault happens.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import Ljos
      Item {
        objectName: "root"
        ErrorBoundary {
          objectName: "boundary"
          fallback: Item { objectName: "sorry" }
          Item { objectName: "content"; property int n: 0 }
        }
        ErrorBoundary {
          objectName: "bare"
          Item { objectName: "plain"; property int n: 0 }
        }
      }
      """

  Scenario: retry brings the content back
    When a native handler of "content" panics with "boom"
    And the method "retry" of "boundary" is called
    Then the object "boundary/content" exists
    And the object "sorry" does not exist
    And the property "failed" of "boundary" is false

  Scenario: retry clears the fault
    When a native handler of "content" panics with "boom"
    And the method "retry" of "boundary" is called
    Then the property "fault.kind" of "boundary" is ""
    And the property "fault.message" of "boundary" is ""

  Scenario: The new content starts from its definition
    When the property "n" of "content" is set to 7
    And a native handler of "content" panics with "boom"
    And the method "retry" of "boundary" is called
    Then the property "n" of "content" is 0

  Scenario: retry counts as a restart
    When a native handler of "content" panics with "boom"
    And the method "retry" of "boundary" is called
    Then the property "restarts" of "boundary" is 1

  Scenario: retry does nothing while the content runs
    When the property "n" of "content" is set to 7
    And the method "retry" of "boundary" is called
    Then the property "n" of "content" is 7
    And the property "restarts" of "boundary" is 0

  Scenario: Without a fallback the content is made again at once
    When the property "n" of "plain" is set to 7
    And a native handler of "plain" panics with "boom"
    Then the object "bare/plain" exists
    And the property "n" of "plain" is 0
    And the property "failed" of "bare" is false

  Scenario: Without a fallback the fault can still be read
    When a native handler of "plain" panics with "boom"
    Then the property "fault.kind" of "bare" is "Panic"
    And the property "restarts" of "bare" is 1
