# Sources:
#   docs/ARCHITECTURE.md §9.3 (faults: JsInterrupted)
#   docs/ARCHITECTURE.md §6.1 (a hard limit on one run of JavaScript)

# In Qt a handler that never returns hangs the application.
@ljos-only
Feature: A handler that never returns is interrupted
  JavaScript that runs past its hard time limit is stopped. That is a
  fault of the boundary around the object, and the application keeps
  running.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property int clicks: 0
        property int total: sum(clicks)
        function spin() { while (true) { } }
        function sum(n) { var t = 0; for (var i = 0; i < 10; i++) t += n; return t }
        function spinReading() { while (true) { try { clicks += 1; total } catch (e) { } } }
        function click() { clicks += 1 }
      }
      """

  Scenario: The application keeps working
    When the method "spin" of "root" is called
    And the method "click" of "root" is called
    Then the property "clicks" of "root" is 1

  Scenario: Without a boundary the application has the fault
    When the method "spin" of "root" is called
    Then the application fault is a "JsInterrupted" matching "interrupted"

  # The interrupt can come while the script is in the binding it reads;
  # the script is stopped there all the same, and cannot catch it.
  Scenario: A handler that keeps reading a binding is interrupted too
    When the method "spinReading" of "root" is called
    Then the application fault is a "JsInterrupted" matching "interrupted"

  Scenario: An exception is logged and is not the application's fault
    When the property "clicks" of "root" is bound to "(function() { throw new Error('no value') })()"
    Then the log has a warning matching "no value"
    And the application has no fault
