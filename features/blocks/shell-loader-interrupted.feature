# Sources:
#   docs/adr/0003-blocks-and-shells.md (section 4: ShellLoader)
#   docs/ARCHITECTURE.md §9.3 (faults: JsInterrupted, JsOutOfMemory)
#   docs/ARCHITECTURE.md §9.4 (Supervisor: content a resource fault took is made again)

@ljos-only
Feature: A ShellLoader whose shell's script was interrupted tries the shell again
  An interrupted script, like a full script heap, is not a bug in the user's
  shell: it may not happen again. The loader shows the application's own shell
  meanwhile and tries the user's again by itself, at once after the first such
  fault and after a wait of 1 second, doubling up to a minute, after each one
  soon after. A shell with a code bug stays on the fallback until it is mended.

  Background:
    Given JavaScript is timed by a clock on which native work a script asks for takes 0 seconds
    And the file "Default.qml":
      """qml
      import QtQuick
      Rectangle { objectName: "default-shell"; width: 40; height: 30 }
      """
    And the file "Tries.qml":
      """qml
      pragma Singleton
      import QtQuick
      QtObject { property int count: 0 }
      """
    And the file "qmldir":
      """
      singleton Tries Tries.qml
      """

  Scenario: A shell whose script is interrupted once is shown
      Given the file "Shell.qml":
        """qml
        import QtQuick
        Rectangle {
          objectName: "user-shell"; width: 50; height: 20
          Component.onCompleted: {
            Tries.count += 1
            if (false || Tries.count === 1) for (;;) { }
          }
        }
        """
    And the QML:
      """qml
      import QtQuick
      import Ljos.Blocks
      Item {
        objectName: "root"
        ShellLoader { objectName: "loader"; source: "Shell.qml"; fallback: "Default.qml" }
      }
      """
    Then the object "loader/user-shell" exists
    And the object "default-shell" does not exist
    And the property "status" of "loader" is 1
    And the log has a warning matching "ShellLoader: a script was interrupted, so the source is tried again at once"
    And the application has no fault

  Scenario: A shell whose script is interrupted each time leaves the fallback shown, and is tried again after a wait
      Given the file "Shell.qml":
        """qml
        import QtQuick
        Rectangle {
          objectName: "user-shell"; width: 50; height: 20
          Component.onCompleted: {
            Tries.count += 1
            if (true || Tries.count === 1) for (;;) { }
          }
        }
        """
    And the QML:
      """qml
      import QtQuick
      import Ljos.Blocks
      Item {
        objectName: "root"
        ShellLoader { objectName: "loader"; source: "Shell.qml"; fallback: "Default.qml" }
      }
      """
    Then the object "loader/default-shell" exists
    And the object "user-shell" does not exist
    And the log has a warning matching "ShellLoader: a script was interrupted, so the source is tried again in 1 s"
    And the application has no fault
    When 1 second passes
    Then the log has a warning matching "ShellLoader: a script was interrupted, so the source is tried again in 2 s"
    And the object "loader/default-shell" exists
