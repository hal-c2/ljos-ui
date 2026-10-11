# Sources:
#   docs/ARCHITECTURE.md §9.3 (faults reach one place)
#   docs/ARCHITECTURE.md §9.4 (Supervisor: UI boundaries, ErrorBoundary)
#   docs/ARCHITECTURE.md §6.2 (binding tiers)

@ljos-only
Feature: A binding that fails is a fault however it is evaluated
  A binding that reads a property of nothing throws a TypeError. Whether
  the engine evaluates the binding itself or hands it to JavaScript, the
  exception is logged the same way and reaches the same boundary.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import Ljos
      Item {
        id: root
        objectName: "root"
        property Item other: root
        property int n: 0
        onNChanged: other = null
        function same(o) { return o }
        ErrorBoundary {
          objectName: "path"; catchExceptions: true
          fallback: Item { objectName: "pathSorry" }
          Item { objectName: "one"; width: root.other.width }
        }
        ErrorBoundary {
          objectName: "sum"; catchExceptions: true
          fallback: Item { objectName: "sumSorry" }
          Item { objectName: "two"; width: root.other.width + 1 }
        }
        ErrorBoundary {
          objectName: "call"; catchExceptions: true
          fallback: Item { objectName: "callSorry" }
          Item { objectName: "three"; width: root.same(root.other).width }
        }
      }
      """

  Scenario: A property path through nothing is a fault of its boundary
    When the property "n" of "root" is set to 1
    Then the object "path/pathSorry" exists
    And the property "failed" of "path" is true

  Scenario: An expression through nothing is a fault of its boundary
    When the property "n" of "root" is set to 1
    Then the object "sum/sumSorry" exists
    And the property "failed" of "sum" is true

  Scenario: A call through nothing is a fault of its boundary
    When the property "n" of "root" is set to 1
    Then the object "call/callSorry" exists
    And the property "failed" of "call" is true

  Scenario: Each is logged with its line and the name of the error
    When the property "n" of "root" is set to 1
    Then the log has a warning matching "Scenario.qml:13: TypeError: Cannot read property 'width' of null"
    And the log has a warning matching "Scenario.qml:18: TypeError: Cannot read property 'width' of null"
    And the log has a warning matching "Scenario.qml:23: TypeError: Cannot read property 'width' of null"
