# Sources:
#   docs/ARCHITECTURE.md §9.3 (faults: a panic is caught at the turn boundary)
#   docs/ARCHITECTURE.md §9.4 (Supervisor)

# Qt has nothing like it: a crash in a C++ type ends the process.
@ljos-only
Feature: A panic in a native type does not end the application
  A type written in Rust can panic while one of its handlers runs. The
  panic ends that handler only. It is logged, it becomes a fault of the
  boundary around the object, and the application keeps running. With no
  boundary around it the application itself is the boundary: nothing is
  destroyed and the host is told.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property int clicks: 0
        Item { objectName: "part"; property int n: 4 }
        function click() { clicks += 1 }
      }
      """

  Scenario: The application starts without a fault
    Then the application has no fault

  Scenario: The application keeps working after a panic
    When a native handler of "part" panics with "index out of range"
    And the method "click" of "root" is called
    Then the property "clicks" of "root" is 1

  Scenario: The panic is logged
    When a native handler of "part" panics with "index out of range"
    Then the log has a warning matching "panic.*index out of range"

  Scenario: Without a boundary the application has the fault
    When a native handler of "part" panics with "index out of range"
    Then the application fault is a "Panic" matching "index out of range"

  Scenario: Without a boundary nothing is destroyed
    When a native handler of "part" panics with "index out of range"
    Then the object "part" exists
    And the property "n" of "part" is 4

  Scenario: The application keeps the latest fault
    When a native handler of "part" panics with "first"
    And a native handler of "root" panics with "second"
    Then the application fault is a "Panic" matching "second"
