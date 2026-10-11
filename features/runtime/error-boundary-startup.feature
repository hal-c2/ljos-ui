# Sources:
#   docs/ARCHITECTURE.md §9.3 (faults: Startup)
#   docs/ARCHITECTURE.md §9.4 (Supervisor: restart intensity, escalation)

@ljos-only
Feature: Content that cannot be made is a fault
  Content that fails while it is made (a required property nobody sets)
  is a fault of the kind Startup. A boundary with a fallback shows it. A
  boundary without one tries again, as often as its restart limit allows,
  and then gives up.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import Ljos
      Item {
        objectName: "root"
        Component { id: needy; Item { objectName: "made"; required property int n } }
        ErrorBoundary {
          objectName: "patient"
          fallback: Item { objectName: "sorry" }
          content: needy
        }
        ErrorBoundary {
          objectName: "stubborn"
          maxRestarts: 2
          content: needy
        }
      }
      """

  Scenario: The fallback shows from the start
    Then the object "patient/sorry" exists
    And the property "failed" of "patient" is true
    And the property "fault.kind" of "patient" is "Startup"

  Scenario: Nothing of the content is left
    Then the object "made" does not exist

  Scenario: A boundary with a fallback waits for retry
    Then the property "restarts" of "patient" is 0

  Scenario: A boundary without a fallback tries as often as it may
    Then the property "restarts" of "stubborn" is 2
    And the property "failed" of "stubborn" is true

  Scenario: Then it gives up and the application has the fault
    Then the application fault is a "Startup" matching "n"
