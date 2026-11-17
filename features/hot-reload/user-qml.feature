# Sources:
#   docs/ARCHITECTURE.md (section 12.1, where the compiler links: a shipped application its users change)
#   features/hot-reload/roots.feature (a user's file over the application's)
#   features/blocks/shell-loader.feature (a user's shell, or the application's own)
# Changing a shipped application is a Ljós extension: Qt has nothing to compare it with.

@ljos-only
Feature: A shipped application its users change while it runs
  An application built ahead of time (`ljos::build`) that lets its users
  change it names one directory for their files (`App::user_qml`). A
  user's file of a name the application has replaces the application's; a
  new one adds a type. Each save is applied as hot reload applies one, with
  the state of the application kept, and a file that does not compile
  leaves the application as it was and is told to the application, never
  shown by Ljós. Until the user has a file nothing is parsed or compiled.
  Files load from that directory only: an address on the network, or one
  that leads out of the application and the directory, is refused and told.

  Background:
    Given the file "Main.qml":
      """qml
      import QtQuick
      import Ljos.Blocks
      Item {
        objectName: "main"
        width: 200; height: 100
        TextInput { objectName: "field"; width: 100; height: 20; focus: true }
        ShellLoader { objectName: "loader"; source: "Shell.qml"; fallback: "DefaultShell.qml" }
        Card { objectName: "card" }
      }
      """
    And the file "DefaultShell.qml":
      """qml
      import QtQuick
      Rectangle { objectName: "default-shell"; width: 40; height: 30 }
      """
    And the file "Card.qml":
      """qml
      import QtQuick
      Rectangle { property string by: "the application"; width: 10; height: 10 }
      """
    And the QML:
      """qml
      import QtQuick
      Item { objectName: "root" }
      """

  Rule: Without a user's file the application runs as it was built

    Scenario: An application whose user has no file compiles nothing and keeps one watch
      When the application runs "Main.qml" built ahead, with the user's directory over it
      Then the user's files are not compiled
      And the user's directory is watched
      And the object "loader/default-shell" exists
      And the property "width" of "card" is 10
      And there is no reload error
      And the application was told of no fault

  Rule: A user's file replaces the application's, or adds a type

    Scenario: A user's file of a name the application has replaces the application's
      Given the user's file "Card.qml":
        """qml
        import QtQuick
        Rectangle { property string by: "the user"; width: 30; height: 10 }
        """
      When the application runs "Main.qml" built ahead, with the user's directory over it
      Then the user's files are compiled
      And the property "by" of "card" is "the user"
      And the property "width" of "card" is 30

    Scenario: A user's new file is the type the application's ShellLoader names
      Given the user's file "Shell.qml":
        """qml
        import QtQuick
        Rectangle { objectName: "user-shell"; width: 50; height: 20 }
        """
      When the application runs "Main.qml" built ahead, with the user's directory over it
      Then the object "loader/user-shell" exists
      And the object "default-shell" does not exist
      And the property "status" of "loader" is 1

    Scenario: A user's first file while the application runs brings it in
      When the application runs "Main.qml" built ahead, with the user's directory over it
      And the user saves the file "Shell.qml" as:
        """qml
        import QtQuick
        Rectangle { objectName: "user-shell"; width: 50; height: 20 }
        """
      Then the user's files are compiled
      And the object "loader/user-shell" exists
      And the object "default-shell" does not exist

  Rule: A save is applied with the state of the application kept

    Scenario: A save keeps what the user typed
      Given the user's file "Card.qml":
        """qml
        import QtQuick
        Rectangle { property string by: "the user"; width: 30; height: 10 }
        """
      When the application runs "Main.qml" built ahead, with the user's directory over it
      And the user types "hello"
      And the user saves the file "Card.qml" as:
        """qml
        import QtQuick
        Rectangle { property string by: "the user"; width: 60; height: 10 }
        """
      Then the property "width" of "card" is 60
      And the property "text" of "field" is "hello"
      And 1 save of the user's files has been applied
      And there is no reload error

    Scenario: A theme in the user's directory is followed
      Given the file "Main.qml":
        """qml
        import QtQuick
        import Ljos.Design
        Item {
          objectName: "main"
          property color accent: Colors.accent
        }
        """
      And the QML:
        """qml
        import QtQuick
        Item { objectName: "root" }
        """
      And the user's file "theme.json":
        """json
        { "Colors": { "accent": "#9c4dc4" } }
        """
      When the application runs "Main.qml" built ahead, with the user's directory over it
      Then the property "accent" of "main" is "#9c4dc4"
      When the user saves the file "theme.json" as:
        """json
        { "Colors": { "accent": "#2e7d32" } }
        """
      Then the property "accent" of "main" is "#2e7d32"
      And there is no reload error

  Rule: A file that does not work leaves the application as it was, and is told

    Scenario: A save that does not compile keeps what runs and is told to the application
      Given the user's file "Card.qml":
        """qml
        import QtQuick
        Rectangle { property string by: "the user"; width: 30; height: 10 }
        """
      When the application runs "Main.qml" built ahead, with the user's directory over it
      And the user types "kept"
      And the user saves the file "Card.qml" as:
        """qml
        import QtQuick
        Rectangle {
          property string by: "the user"
          NoSuchThing { }
        }
        """
      Then the reload error is in "Card.qml" at line 4
      And the application was told of a fault matching "Card\.qml:4"
      And the property "by" of "card" is "the user"
      And the property "width" of "card" is 30
      And the property "text" of "field" is "kept"

    Scenario: A user's shell that does not compile at the start gives way to the application's
      Given the user's file "Shell.qml":
        """qml
        import QtQuick
        Rectangle {
          NoSuchThing { }
        }
        """
      When the application runs "Main.qml" built ahead, with the user's directory over it
      Then the object "loader/default-shell" exists
      And the property "status" of "loader" is 2
      And the application was told of a fault matching "Shell\.qml:3"

  Rule: What the application was built from is what a user's file compiles against

    Scenario: A file the build compiled from another text than the one it carries is compiled again
      Given the user's file "Shell.qml":
        """qml
        import QtQuick
        Rectangle { objectName: "user-shell" }
        """
      When the application runs "Main.qml" built ahead from an older "Card.qml", with the user's directory over it:
        """qml
        import QtQuick
        Rectangle { property string by: "an older build"; width: 99; height: 10 }
        """
      Then the property "by" of "card" is "the application"
      And the property "width" of "card" is 10

  Rule: Files load from the user's directory and the application only

    Scenario Outline: An address that leads elsewhere is refused and told
      Given the user's file "Shell.qml":
        """qml
        import QtQuick
        Item {
          objectName: "user-shell"
          Loader { source: "<address>" }
        }
        """
      When the application runs "Main.qml" built ahead, with the user's directory over it
      Then the object "loader/user-shell" exists
      And the application was told of a fault matching "refused: .*<told>"
      And the reload error matches "refused"

      Examples:
        | address                      | told                                  |
        | https://example.com/Evil.qml | a network address                     |
        | ../../etc/Evil.qml           | names a place outside the application |
        | /etc/Evil.qml                | names a place outside the application |

    Scenario: A user's file that links out of the directory is refused and told
      Given the user's file "Shell.qml" links to a file outside the user's directory
      When the application runs "Main.qml" built ahead, with the user's directory over it
      Then the object "loader/default-shell" exists
      And the object "outside" does not exist
      And the application was told of a fault matching "Shell\.qml: a link that leads out"
