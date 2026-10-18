# Sources:
#   docs/ARCHITECTURE.md §9.3 (faults: JsInterrupted)
#   docs/ARCHITECTURE.md §6.1 (a hard limit on one run of JavaScript)
#   docs/ARCHITECTURE.md §5.4 (tier 1, cross-checked against tier 2)

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
        property int floor: Math.max(clicks, 1) + Math.max(clicks, 2) + Math.max(clicks, 3) + Math.max(clicks, 4)
          + Math.max(clicks, 5) + Math.max(clicks, 6) + Math.max(clicks, 7) + Math.max(clicks, 8)
        function spinFloor() { var n = 0; while (true) { clicks += 1; n += floor } }
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

  # With the cross-check on, tier 2 runs each binding tier 1 ran, to compare
  # them. The interrupt can come while it does: the binding did not throw,
  # so the tiers do not differ. `floor` is long so that the interrupt comes
  # there more often than not.
  Scenario: An interrupt during the cross-check of a binding is not a difference
    Given the tier 1 cross-check is on
    When the method "spinFloor" of "root" is called
    Then the application fault is a "JsInterrupted" matching "interrupted"
    And the log has no warning matching "tier 1 and tier 2 differ"
