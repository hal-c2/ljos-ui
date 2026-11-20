# Sources:
#   docs/ARCHITECTURE.md §9.3 (faults: JsInterrupted, JsOutOfMemory)
#   docs/ARCHITECTURE.md §9.4 (Supervisor: restart intensity, escalation; content a resource fault took is made again)

# Qt has no time limit on JavaScript: a script that never returns hangs the application.
@ljos-only
Feature: An ErrorBoundary whose content's script was interrupted makes it again, and never gives up
  An interrupted script, like a full script heap, is not a bug in the content:
  it may not happen again. The boundary shows its fallback and makes its
  content again by itself, at once after the first such fault and after a wait
  of 1 second, doubling up to a minute, after each one soon after. Such faults
  do not count toward `maxRestarts`, so the boundary never gives up on them
  and they never become the fault of the boundary around it. A code bug still
  counts, as before.

  Background:
    Given JavaScript is timed by a clock on which native work a script asks for takes 0 seconds

  Scenario: Content whose script is interrupted once is made again
    Given the QML:
      """qml
      import QtQuick
      import Ljos
      ErrorBoundary {
        objectName: "outer"
        fallback: Item { objectName: "outerSorry" }
        Item {
          id: root
          objectName: "root"
          property int tries: 0
          property bool always: false
          ErrorBoundary {
            objectName: "inner"
            maxRestarts: 1
            within: 60000
            fallback: Item { objectName: "sorry" }
            Item {
              objectName: "content"
              Component.onCompleted: {
                root.tries += 1
                if (root.always || root.tries === 1) for (;;) { }
              }
            }
          }
        }
      }
      """
    Then the property "tries" of "root" is 2
    And the object "inner/content" exists
    And the log has a warning matching "ErrorBoundary: a script was interrupted, so its content is made again at once"
    And the property "failed" of "outer" is false
    And the application has no fault

  Scenario: Content interrupted again and again waits longer each time and never escalates
    Given the QML:
      """qml
      import QtQuick
      import Ljos
      ErrorBoundary {
        objectName: "outer"
        fallback: Item { objectName: "outerSorry" }
        Item {
          id: root
          objectName: "root"
          property int tries: 0
          property bool always: true
          ErrorBoundary {
            objectName: "inner"
            maxRestarts: 1
            within: 60000
            fallback: Item { objectName: "sorry" }
            Item {
              objectName: "content"
              Component.onCompleted: {
                root.tries += 1
                if (root.always || root.tries === 1) for (;;) { }
              }
            }
          }
        }
      }
      """
    Then the property "tries" of "root" is 2
    And the object "inner/sorry" exists
    And the log has a warning matching "ErrorBoundary: a script was interrupted, so its content is made again in 1 s"
    When 999 milliseconds pass
    Then the property "tries" of "root" is 2
    When 1 millisecond passes
    Then the property "tries" of "root" is 3
    And the log has a warning matching "made again in 2 s"
    When 2 seconds pass
    Then the property "tries" of "root" is 4
    And the object "inner" exists
    And the object "outerSorry" does not exist
    And the property "failed" of "outer" is false
    And the application has no fault

  Scenario: Content whose script stops looping comes back by itself
    Given the QML:
      """qml
      import QtQuick
      import Ljos
      ErrorBoundary {
        objectName: "outer"
        fallback: Item { objectName: "outerSorry" }
        Item {
          id: root
          objectName: "root"
          property int tries: 0
          property bool always: true
          ErrorBoundary {
            objectName: "inner"
            maxRestarts: 1
            within: 60000
            fallback: Item { objectName: "sorry" }
            Item {
              objectName: "content"
              Component.onCompleted: {
                root.tries += 1
                if (root.always || root.tries === 1) for (;;) { }
              }
            }
          }
        }
      }
      """
    When the property "always" of "root" is set to false
    And 1 second passes
    Then the property "tries" of "root" is 3
    And the object "inner/content" exists
    And the object "inner/sorry" does not exist
    And the application has no fault
