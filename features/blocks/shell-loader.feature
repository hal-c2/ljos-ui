# Sources:
#   docs/adr/0003-blocks-and-shells.md (section 4: ShellLoader)
#   docs/ARCHITECTURE.md §12.1 (the default shell is the fallback; the old generation stays on error)
#   docs/research/07-hal-c2-requirements.md (§2.2: a user's shell with a default to fall back to)

@ljos-only
Feature: A ShellLoader shows a user's shell, or the application's own when the user's does not work
  An application that lets its users replace its whole shell loads theirs
  with a `ShellLoader`. When the user's file does not compile, or throws
  while it is made, the loader shows the application's own shell instead
  and `error` says what went wrong and where. Mending the file while the
  application runs under `--watch` brings the user's shell back.

  Background:
    Given the file "Default.qml":
      """qml
      import QtQuick
      Rectangle { objectName: "default-shell"; width: 40; height: 30 }
      """

  Rule: A shell that works is shown

    Scenario: The user's shell is shown
      Given the file "Shell.qml":
        """qml
        import QtQuick
        Rectangle { objectName: "user-shell"; width: 50; height: 20 }
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
      And the property "error" of "loader" is ""
      And the property "status" of "loader" is 1
      And the property "width" of "loader" is 50

  Rule: A shell that does not work gives way to the fallback

    Scenario: A shell that does not compile falls back and says where the mistake is
      Given the file "Shell.qml":
        """qml
        import QtQuick
        Rectangle {
          NoSuchThing { }
        }
        """
      And the QML:
        """qml
        import QtQuick
        import Ljos.Blocks
        Item {
          objectName: "root"
          ShellLoader { id: loader; objectName: "loader"; source: "Shell.qml"; fallback: "Default.qml" }
          property bool named: /Shell\.qml:3:\d+: .*NoSuchThing/.test(loader.error)
        }
        """
      Then the object "loader/default-shell" exists
      And the property "named" of "root" is true
      And the property "status" of "loader" is 2
      And the log has a warning matching "Shell\.qml:3"

    Scenario: A shell that throws while it is made falls back and says where it threw
      Given the file "Shell.qml":
        """qml
        import QtQuick
        Rectangle {
          objectName: "user-shell"
          Component.onCompleted: { throw new Error("no shell today") }
        }
        """
      And the QML:
        """qml
        import QtQuick
        import Ljos.Blocks
        Item {
          objectName: "root"
          ShellLoader { id: loader; objectName: "loader"; source: "Shell.qml"; fallback: "Default.qml" }
          property bool named: /Shell\.qml:4: .*no shell today/.test(loader.error)
        }
        """
      Then the object "loader/default-shell" exists
      And the object "user-shell" does not exist
      And the property "named" of "root" is true

    Scenario: A shell that fails after it was made falls back
      Given the file "Shell.qml":
        """qml
        import QtQuick
        Rectangle { objectName: "user-shell"; Item { objectName: "part" } }
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
      When a native handler of "part" panics with "shell broke"
      Then the object "loader/default-shell" exists
      And the object "user-shell" does not exist
      And the property "status" of "loader" is 2
      And the application has no fault

    Scenario: A fallback that fails too leaves nothing shown and says both
      Given the file "Shell.qml":
        """qml
        import QtQuick
        Rectangle { NoSuchThing { } }
        """
      And the file "Default.qml":
        """qml
        import QtQuick
        Rectangle { AlsoMissing { } }
        """
      And the QML:
        """qml
        import QtQuick
        import Ljos.Blocks
        Item {
          objectName: "root"
          ShellLoader { id: loader; objectName: "loader"; source: "Shell.qml"; fallback: "Default.qml" }
          property bool both: /Shell\.qml:2.*NoSuchThing[^]*Default\.qml:2.*AlsoMissing/.test(loader.error)
        }
        """
      Then the property "both" of "root" is true
      And the property "status" of "loader" is 3
      And the property "item" of "loader" is null

    Scenario: A new source is tried and clears the error when it works
      Given the file "Shell.qml":
        """qml
        import QtQuick
        Rectangle { NoSuchThing { } }
        """
      And the file "Other.qml":
        """qml
        import QtQuick
        Rectangle { objectName: "other-shell" }
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
      When the property "source" of "loader" is set to "Other.qml"
      Then the object "loader/other-shell" exists
      And the object "default-shell" does not exist
      And the property "error" of "loader" is ""
      And the property "status" of "loader" is 1

  Rule: Editing the shell while the application runs

    Scenario: Mending the shell's file replaces the fallback
      Given the file "Shell.qml":
        """qml
        import QtQuick
        Rectangle { NoSuchThing { } }
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
      When the file "Shell.qml" is changed to:
        """qml
        import QtQuick
        Rectangle { objectName: "user-shell" }
        """
      And a reload happens
      Then there is no reload error
      And the object "loader/user-shell" exists
      And the object "default-shell" does not exist
      And the property "error" of "loader" is ""

    Scenario: A shell that an edit breaks stays as it was, and the reload says why
      Given the file "Shell.qml":
        """qml
        import QtQuick
        Rectangle { objectName: "user-shell" }
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
      When the file "Shell.qml" is changed to:
        """qml
        import QtQuick
        Rectangle { objectName: "user-shell"; NoSuchThing { } }
        """
      And a reload happens
      Then the reload error is in "Shell.qml" at line 2
      And the object "loader/user-shell" exists
      And the object "default-shell" does not exist
