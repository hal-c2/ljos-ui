# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-loader.html#status-prop
#   docs/ARCHITECTURE.md §9.4 (Supervisor: UI boundaries, Loader)

# Qt has no faults: a crash below a Loader ends the process.
@ljos-only
Feature: A Loader is a boundary
  A fault in what a Loader loaded destroys the loaded item and nothing
  else. The Loader reports `Loader.Error` and loads again when it is
  told to.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        Item { objectName: "neighbour" }
        Component { id: page; Item { objectName: "page"; Item { objectName: "deep" } } }
        Loader { id: loader; objectName: "loader"; sourceComponent: page }
        function again() { loader.active = false; loader.active = true }
      }
      """

  Scenario: A fault in the loaded item unloads it
    When a native handler of "deep" panics with "boom"
    Then the object "page" does not exist
    And the property "item" of "loader" is null

  Scenario: The Loader reports an error
    When a native handler of "deep" panics with "boom"
    Then the property "status" of "loader" is 3

  Scenario: What is outside the Loader lives on
    When a native handler of "deep" panics with "boom"
    Then the object "neighbour" exists
    And the object "loader" exists
    And the application has no fault

  Scenario: The Loader loads again when it is switched off and on
    When a native handler of "deep" panics with "boom"
    And the method "again" of "root" is called
    Then the object "loader/page" exists
    And the property "status" of "loader" is 1

  Scenario: A fault outside the Loader leaves its item alone
    When a native handler of "neighbour" panics with "boom"
    Then the object "loader/page" exists
    And the property "status" of "loader" is 1
