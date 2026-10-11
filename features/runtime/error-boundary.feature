# Sources:
#   docs/ARCHITECTURE.md §9.4 (Supervisor: UI boundaries, ErrorBoundary)
#   docs/ARCHITECTURE.md §9.3 (faults)

@ljos-only
Feature: ErrorBoundary shows a fallback when what is inside it fails
  An ErrorBoundary makes the item written inside it. A fault below it
  destroys that item and nothing else, and the boundary shows its
  fallback in its place.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import Ljos
      Item {
        objectName: "root"
        Item { objectName: "neighbour" }
        ErrorBoundary {
          id: boundary
          objectName: "boundary"
          fallback: Text { objectName: "sorry"; text: "Something went wrong" }
          Rectangle {
            objectName: "content"; width: 100; height: 50
            Item { objectName: "deep" }
          }
        }
      }
      """

  Scenario: It shows its content
    Then the object "boundary/content" exists
    And the object "sorry" does not exist
    And the property "failed" of "boundary" is false

  Scenario: It is as large as its content
    Then the property "width" of "boundary" is 100
    And the property "height" of "boundary" is 50

  Scenario: A fault in its content destroys the content
    When a native handler of "content" panics with "boom"
    Then the object "content" does not exist
    And the object "deep" does not exist
    And the property "failed" of "boundary" is true

  Scenario: A fault deep in its content is its fault too
    When a native handler of "deep" panics with "boom"
    Then the object "content" does not exist
    And the property "failed" of "boundary" is true

  Scenario: The fallback takes the content's place
    When a native handler of "content" panics with "boom"
    Then the object "boundary/sorry" exists
    And the property "text" of "sorry" is "Something went wrong"

  Scenario: What is outside the boundary lives on
    When a native handler of "content" panics with "boom"
    Then the object "neighbour" exists
    And the application has no fault

  Scenario: A fault outside the boundary is not its fault
    When a native handler of "neighbour" panics with "boom"
    Then the object "boundary/content" exists
    And the property "failed" of "boundary" is false
