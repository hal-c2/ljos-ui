# Sources:
#   docs/ARCHITECTURE.md (section 12.1, where the compiler links: a shipped application its users change)
#   features/hot-reload/roots.feature (a user's file over the application's)
#   features/blocks/shell-loader.feature (a user's shell, or the application's own)
# Changing a shipped application is a Ljós extension: Qt has nothing to compare it with.

@ljos-only
Feature: A shipped application its users change while it runs
  An application built ahead of time (`ljos::build`) that lets its users
  change it names one directory for their files (`App::user_qml`), and
  how its users lay it out when they already keep files there. A
  user's file of a name the application has replaces the application's; a
  new one adds a type. Each save is applied as hot reload applies one, with
  the state of the application kept, and a file that does not compile
  leaves the application as it was and is told to the application, never
  shown by Ljós. Asked, the application reads the files again, as a save
  the watch saw. Until the user has a file nothing is parsed or compiled.
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
    And the file "Second.qml":
      """qml
      import QtQuick
      Item { objectName: "second"; Card { objectName: "second-card" } }
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

    # Reading the directory changes nothing in it: a watch that took its
    # own listing for a change would wake the application for ever. Exactly
    # 0 is stable: the read step first takes what the start woke it for, and
    # after that only the user's directory wakes this session; no timer or
    # animation does.
    Scenario: A user's directory with no file, read and left alone, leaves the application idle
      When the application runs "Main.qml" built ahead, with the user's directory over it
      And the user's directory is read
      And 1 second passes with the application answering every wake
      Then the application was woken at most 0 times while it was left alone
      And the user's files are not compiled
      And the user's directory is watched

    Scenario: A shell the user has not written is named where the user would write it
      When the application runs "Main.qml" built ahead, with the user's directory over it
      Then the object "loader/default-shell" exists
      And the log has a warning matching "^ShellLoader: <dir>/Shell\.qml: No such file or directory$" with "<dir>" for the user's directory
      And the log has no warning matching "file:///Shell\.qml"

    Scenario: A shell an application built without a user's directory lacks is named as the application names it
      When the application runs "Main.qml" built ahead, without a user's directory
      Then the object "loader/default-shell" exists
      And the log has a warning matching "^ShellLoader: Shell\.qml: No such file or directory$"
      And the log has no warning matching "file:///"

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

    Scenario: A user's directory removed and made again is watched, and a first file in it brings it in
      When the application runs "Main.qml" built ahead, with the user's directory over it
      And the user's directory is removed and made again
      Then the user's directory is watched
      When the user saves the file "Shell.qml" as:
        """qml
        import QtQuick
        Rectangle { objectName: "user-shell"; width: 50; height: 20 }
        """
      Then the user's files are compiled
      And the object "loader/user-shell" exists

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

    Scenario: What a save no longer gives goes back to where a fresh start has it, and what the user typed stays
      Given the user's file "Shell.qml":
        """qml
        import QtQuick
        Rectangle {
          objectName: "user-shell"; width: 200; height: 100; color: "green"
          Text { objectName: "title"; text: "Hi"; width: 40; height: 20; font.bold: true; anchors.centerIn: parent }
        }
        """
      When the application runs "Main.qml" built ahead, with the user's directory over it
      And the user types "hello"
      Then the property "y" of "loader/user-shell/title" is 40
      When the user saves the file "Shell.qml" as:
        """qml
        import QtQuick
        Rectangle {
          objectName: "user-shell"; width: 200; height: 100
          Text { objectName: "title"; text: "Hi"; width: 40; height: 20 }
        }
        """
      Then the property "x" of "loader/user-shell/title" is 0
      And the property "y" of "loader/user-shell/title" is 0
      And the property "font.bold" of "loader/user-shell/title" is false
      And the property "color" of "loader/user-shell" is "#ffffff"
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

    Scenario: A save that adds a view whose first page reads its parent is applied without a word
      Given the user's file "Shell.qml":
        """qml
        import QtQuick
        import QtQuick.Controls
        Item { objectName: "user-shell"; width: 200; height: 100 }
        """
      When the application runs "Main.qml" built ahead, with the user's directory over it
      And the user saves the file "Shell.qml" as:
        """qml
        import QtQuick
        import QtQuick.Controls
        Item {
          objectName: "user-shell"; width: 200; height: 100
          StackView {
            anchors.fill: parent
            initialItem: Rectangle {
              objectName: "page"
              anchors.left: parent.left; anchors.right: parent.right; anchors.top: parent.top
              height: 60
            }
          }
        }
        """
      Then the property "width" of "page" is 200
      And the log has no warning matching "TypeError"
      And the application was told of no fault
      And 1 save of the user's files has been applied
      And there is no reload error

  Rule: The user's files are read again when the application asks, as a save the watch saw

    Where a save goes unseen (the application keeps no watch, or the watch
    was lost), the application asks for the files to be read again: from
    QML (`ShellLoader.reload()`) or from Rust (`UserSession::read_again`).
    What was saved is applied as a save the watch saw is, with the state of
    the application kept; one that does not compile is told as one is.
    Files that are as they were keep what runs, and that is said. An
    application that reads them only when it starts keeps what runs too.
    Unless a file failed, a ShellLoader that asked and shows its fallback
    tries its source again, as it does after a new theme its shell works
    with, which makes nothing again.

    Scenario: A save no watch saw is applied once a ShellLoader asks for the files to be read again
      Given the user's file "Card.qml":
        """qml
        import QtQuick
        Rectangle { property string by: "the user"; width: 30; height: 10 }
        """
      And the application reads its user's files only when it starts and when asked
      When the application runs "Main.qml" built ahead, with the user's directory over it
      And the user types "kept"
      And the user saves the file "Shell.qml" as:
        """qml
        import QtQuick
        Rectangle {
          objectName: "user-shell"; width: 50; height: 20
          Component.onCompleted: console.log("the user's shell is made")
        }
        """
      Then the user's directory is not watched
      And the object "loader/default-shell" exists
      When the method "reload" of "loader" is called
      Then the object "loader/user-shell" exists
      And the object "default-shell" does not exist
      And the property "status" of "loader" is 1
      And the property "error" of "loader" is ""
      And the property "text" of "field" is "kept"
      And the log has 1 info matching "the user's shell is made"
      And 1 save of the user's files has been applied
      And the user's directory is not watched

    Scenario: A user's first file no watch saw is brought in once the application asks for the files to be read again
      Given the application reads its user's files only when it starts and when asked
      When the application runs "Main.qml" built ahead, with the user's directory over it
      And the user saves the file "Card.qml" as:
        """qml
        import QtQuick
        Rectangle { property string by: "the user"; width: 60; height: 10 }
        """
      Then the user's files are not compiled
      And the property "width" of "card" is 10
      When the application asks for its user's files to be read again
      Then the user's files are compiled
      And the property "by" of "card" is "the user"
      And the property "width" of "card" is 60
      And there is no reload error

    Scenario: A save that does not compile, read again when asked, keeps what runs and is told
      Given the user's file "Card.qml":
        """qml
        import QtQuick
        Rectangle { property string by: "the user"; width: 30; height: 10 }
        """
      And the application reads its user's files only when it starts and when asked
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
      And the method "reload" of "loader" is called
      Then the reload error is in "Card.qml" at line 4
      And the application was told of a fault matching "Card\.qml:4"
      And the property "width" of "card" is 30
      And the property "text" of "field" is "kept"
      And the object "loader/default-shell" exists

    Scenario: Files read again that are as they were keep the user's shell and what the user typed, and say so
      Given the user's file "Shell.qml":
        """qml
        import QtQuick
        Rectangle {
          objectName: "user-shell"; width: 50; height: 20
          Component.onCompleted: console.log("the user's shell is made")
        }
        """
      When the application runs "Main.qml" built ahead, with the user's directory over it
      And the user types "kept"
      And the method "reload" of "loader" is called
      Then the user's files read again were as they were
      And the object "loader/user-shell" exists
      And the property "status" of "loader" is 1
      And the property "text" of "field" is "kept"
      And the log has 1 info matching "the user's shell is made"
      And 0 saves of the user's files have been applied
      And there is no reload error

    Scenario: Files read again before the user has any are as they were, and the application's shell stays
      When the application runs "Main.qml" built ahead, with the user's directory over it
      And the application asks for its user's files to be read again
      Then the user's files read again were as they were
      And the user's files are not compiled
      And the object "loader/default-shell" exists
      And the property "status" of "loader" is 2

    Scenario: An application that reads its user's files only when it starts keeps what runs when asked, and says so
      Given the user's file "Card.qml":
        """qml
        import QtQuick
        Rectangle { property string by: "the user"; width: 30; height: 10 }
        """
      And the application reads its user's files only when it starts
      When the application runs "Main.qml" built ahead, with the user's directory over it
      And the user saves the file "Card.qml" as:
        """qml
        import QtQuick
        Rectangle { property string by: "the user"; width: 60; height: 10 }
        """
      And the application asks for its user's files to be read again
      Then the user's files read again were as they were
      And the property "width" of "card" is 30
      And 0 saves of the user's files have been applied
      And the user's directory is not watched

    Scenario: A user's shell that did not work is shown once a new theme it works with is read again when asked
      Given the user's file "theme.json":
        """json
        { "Colors": { "accent": "#9c4dc4" } }
        """
      And the user's file "Shell.qml":
        """qml
        import QtQuick
        import Ljos.Design
        Rectangle {
          id: shell
          objectName: "user-shell"
          property var size
          width: Qt.colorEqual(Colors.accent, "#2e7d32") ? 50 : shell.size.width
          height: 20
        }
        """
      And the application reads its user's files only when it starts and when asked
      When the application runs "Main.qml" built ahead, with the user's directory over it
      Then the object "loader/default-shell" exists
      And the property "status" of "loader" is 2
      When the user saves the file "theme.json" as:
        """json
        { "Colors": { "accent": "#2e7d32" } }
        """
      And the method "reload" of "loader" is called
      Then the object "loader/user-shell" exists
      And the object "default-shell" does not exist
      And the property "status" of "loader" is 1
      And the property "error" of "loader" is ""
      And the property "width" of "loader/user-shell" is 50

  Rule: A user's shell that is a Window is the application's window

    An application whose root is a `ShellLoader` shows a shell that is a
    `Window` as its window, where each `Window` is a platform window. A
    save of the user's shell is applied to it in that window, as a save is
    applied to any file; its removal puts the application's shell in that
    window at the size the application's shell declares.

    Background:
      Given each Window has a platform window of its own
      And the file "DefaultWindow.qml":
        """qml
        import QtQuick
        Window { objectName: "default-window"; width: 300; height: 240; visible: true }
        """
      And the file "App.qml":
        """qml
        import Ljos.Blocks
        ShellLoader { objectName: "loader"; source: "Shell.qml"; fallback: "DefaultWindow.qml" }
        """

    Scenario: A user's shell that is a Window is the only window
      Given the user's file "Shell.qml":
        """qml
        import QtQuick
        Window { objectName: "user-window"; width: 200; height: 100; visible: true }
        """
      When the application runs "App.qml" built ahead in a window of its own, with the user's directory over it
      Then the application's windows are "user-window"
      And the property "width" of "user-window" is 200
      And the property "status" of "loader" is 1

    Scenario: A save of the user's Window shell is applied in the same window
      Given the user's file "Shell.qml":
        """qml
        import QtQuick
        Window { objectName: "user-window"; width: 200; height: 100; visible: true }
        """
      When the application runs "App.qml" built ahead in a window of its own, with the user's directory over it
      And the user saves the file "Shell.qml" as:
        """qml
        import QtQuick
        Window {
          objectName: "user-window"; width: 200; height: 100; visible: true
          title: "Edited"
          Item { objectName: "part" }
        }
        """
      Then 1 save of the user's files has been applied
      And there is no reload error
      And the application's windows are "user-window"
      And the property "title" of "user-window" is "Edited"
      And the object "loader/part" exists

    Scenario: The user's Window shell removed gives the window to the application's, at the size it declares
      Given the user's file "Shell.qml":
        """qml
        import QtQuick
        Window { objectName: "user-window"; width: 200; height: 100; visible: true }
        """
      When the application runs "App.qml" built ahead in a window of its own, with the user's directory over it
      And the user removes the file "Shell.qml"
      Then the application's windows are "default-window"
      And the property "width" of "default-window" is 300
      And the property "height" of "default-window" is 240
      And the property "status" of "loader" is 2
      And there is no reload error

    Scenario: A user's shell that is an item, saved and removed, leaves the application's window
      When the application runs "App.qml" built ahead in a window of its own, with the user's directory over it
      And the user saves the file "Shell.qml" as:
        """qml
        import QtQuick
        Rectangle { objectName: "user-shell" }
        """
      Then the application's windows are "loader"
      And the object "loader/user-shell" exists
      When the user removes the file "Shell.qml"
      Then the application's windows are "default-window"
      And the property "width" of "default-window" is 300
      And the application was told of no fault

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

    Scenario: A save that links out of the directory keeps what runs and is told
      Given the user's file "Shell.qml":
        """qml
        import QtQuick
        Rectangle { objectName: "user-shell"; width: 50; height: 20 }
        """
      When the application runs "Main.qml" built ahead, with the user's directory over it
      And the user types "kept"
      And the user saves the file "Shell.qml" as a link to a file outside the user's directory
      Then the object "loader/user-shell" exists
      And the object "default-shell" does not exist
      And the object "outside" does not exist
      And the application was told of a fault matching "Shell\.qml: a link that leads out"
      And the property "text" of "field" is "kept"

    Scenario: A theme saved as a link out of the directory keeps the theme that runs and is told
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
      And the user saves the file "theme.json" as a link to a file outside the user's directory
      Then the property "accent" of "main" is "#9c4dc4"
      And the application was told of a fault matching "theme[.]json: a link that leads out"

    Scenario: A user's file removed gives way to the application's
      Given the user's file "Card.qml":
        """qml
        import QtQuick
        Rectangle { property string by: "the user"; width: 30; height: 10 }
        """
      When the application runs "Main.qml" built ahead, with the user's directory over it
      And the user removes the file "Card.qml"
      Then the property "by" of "card" is "the application"
      And the property "width" of "card" is 10
      And there is no reload error
      And the application was told of no fault

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

    Scenario: A user's script runs in the application's files that import it
      Given the file "calc.js":
        """js
        function by() { return "the application" }
        """
      And the file "Main.qml":
        """qml
        import QtQuick
        import "calc.js" as Calc
        Item { objectName: "main"; property string by: Calc.by() }
        """
      And the QML:
        """qml
        import QtQuick
        Item { objectName: "root" }
        """
      And the user's file "calc.js":
        """js
        function by() { return "the user" }
        """
      When the application runs "Main.qml" built ahead, with the user's directory over it
      Then the property "by" of "main" is "the user"
      And the application was told of no fault

    Scenario: A user's script that does not parse is told, and what imports it runs as built
      Given the file "calc.js":
        """js
        function by() { return "the application" }
        """
      And the file "Main.qml":
        """qml
        import QtQuick
        import "calc.js" as Calc
        Item { objectName: "main"; property string by: Calc.by() }
        """
      And the QML:
        """qml
        import QtQuick
        Item { objectName: "root" }
        """
      And the user's file "calc.js":
        """js
        function by( { return "the user" }
        """
      When the application runs "Main.qml" built ahead, with the user's directory over it
      Then the property "by" of "main" is "the application"
      And the application was told of a fault matching "calc\.js"

    Scenario: A user's script that does not parse behind another script is told, and what imports them runs as built
      Given the file "calc.js":
        """js
        function by() { return "the application" }
        """
      And the file "names.js":
        """js
        .import "calc.js" as Calc
        function by() { return Calc.by() }
        """
      And the file "Main.qml":
        """qml
        import QtQuick
        import "names.js" as Names
        Item { objectName: "main"; property string by: Names.by() }
        """
      And the QML:
        """qml
        import QtQuick
        Item { objectName: "root" }
        """
      And the user's file "calc.js":
        """js
        function by( { return "the user" }
        """
      When the application runs "Main.qml" built ahead, with the user's directory over it
      Then the property "by" of "main" is "the application"
      And the application was told of a fault matching "calc\.js"

    Scenario: A user's script that does not parse leaves alone a user's file whose scripts only name it
      Given the file "calc.js":
        """js
        function by() { return "the application" }
        """
      And the file "names.js":
        """js
        // Not calc.js: a name of its own.
        function by() { return "names" }
        """
      And the file "Main.qml":
        """qml
        import QtQuick
        import "names.js" as Names
        Item { objectName: "main"; property string by: Names.by() }
        """
      And the QML:
        """qml
        import QtQuick
        Item { objectName: "root" }
        """
      And the user's file "calc.js":
        """js
        function by( { return "the user" }
        """
      And the user's file "Main.qml":
        """qml
        import QtQuick
        import "names.js" as Names
        Item { objectName: "main"; property string by: Names.by() + " and the user" }
        """
      When the application runs "Main.qml" built ahead, with the user's directory over it
      Then the property "by" of "main" is "names and the user"
      And the application was told of a fault matching "calc\.js"

  Rule: An application built without its sources runs as built, and says why once

    Scenario: A user's file over an application built without its sources is told once and nothing is watched
      Given the user's file "Card.qml":
        """qml
        import QtQuick
        Rectangle { property string by: "the user"; width: 30; height: 10 }
        """
      When the application runs "Main.qml" built ahead without its sources, with the user's directory over it
      Then the user's files are not compiled
      And the user's directory is not watched
      And the property "by" of "card" is "the application"
      And the application was told of 1 fault matching "carries no sources"

    Scenario: A user's first file over an application built without its sources is told once
      When the application runs "Main.qml" built ahead without its sources, with the user's directory over it
      Then the user's directory is watched
      When the user saves the file "Card.qml" as:
        """qml
        import QtQuick
        Rectangle { property string by: "the user"; width: 30; height: 10 }
        """
      And the user saves the file "Shell.qml" as:
        """qml
        import QtQuick
        Rectangle { objectName: "user-shell" }
        """
      Then the user's files are not compiled
      And the user's directory is not watched
      And the property "by" of "card" is "the application"
      And the application was told of 1 fault matching "carries no sources"

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
        | address                      | told                                                                |
        | https://example.com/Evil.qml | a network address                                                   |
        | ../Escape.qml                | [.][.]/Escape[.]qml: names a place outside the application          |
        | ../../etc/Evil.qml           | [.][.]/[.][.]/etc/Evil[.]qml: names a place outside the application |
        | /etc/Evil.qml                | names a place outside the application                               |

    Scenario: A step up out of the user's directory is refused where the application has the file
      Given the file "Escape.qml":
        """qml
        import QtQuick
        Rectangle { objectName: "escaped" }
        """
      And the user's file "Shell.qml":
        """qml
        import QtQuick
        Item {
          objectName: "user-shell"
          Loader { source: "../Escape.qml" }
          Loader { source: Qt.resolvedUrl("../Escape.qml") }
          Component.onCompleted: Qt.createComponent("../Escape.qml")
        }
        """
      When the application runs "Main.qml" built ahead, with the user's directory over it
      Then the object "loader/user-shell" exists
      And the object "escaped" does not exist
      And the application was told of a fault matching "refused: \.\./Escape\.qml: names a place outside"
      And the reload error matches "refused"

    Scenario: A step up saved while the application runs is refused
      Given the file "Escape.qml":
        """qml
        import QtQuick
        Rectangle { objectName: "escaped" }
        """
      And the user's file "Shell.qml":
        """qml
        import QtQuick
        Item { objectName: "user-shell" }
        """
      When the application runs "Main.qml" built ahead, with the user's directory over it
      And the user saves the file "Shell.qml" as:
        """qml
        import QtQuick
        Item {
          objectName: "user-shell"
          Loader { source: "../Escape.qml" }
          Loader { source: Qt.resolvedUrl("../Escape.qml") }
          Component.onCompleted: Qt.createComponent("../Escape.qml")
        }
        """
      Then the object "loader/user-shell" exists
      And the object "escaped" does not exist
      And the application was told of a fault matching "refused: \.\./Escape\.qml: names a place outside"

    Scenario: A step up that stays in the user's directory is loaded
      Given the user's file "Part.qml":
        """qml
        import QtQuick
        Rectangle { objectName: "part" }
        """
      And the user's file "pages/Page.qml":
        """qml
        import QtQuick
        Item { objectName: "page"; Loader { source: "../Part.qml" } }
        """
      And the user's file "Shell.qml":
        """qml
        import QtQuick
        Item { objectName: "user-shell"; Loader { source: "pages/Page.qml" } }
        """
      When the application runs "Main.qml" built ahead, with the user's directory over it
      Then the object "part" exists
      And the application was told of no fault

    Scenario: A user's file that links out of the directory is refused and told
      Given the user's file "Shell.qml" links to a file outside the user's directory
      When the application runs "Main.qml" built ahead, with the user's directory over it
      Then the object "loader/default-shell" exists
      And the object "outside" does not exist
      And the application was told of a fault matching "Shell\.qml: a link that leads out"

  Rule: A window the application opens beside the first follows the user's files

    A save patched in place, a user's file removed among them, reaches every
    object made of the file, in whichever window. A change no patch in place
    can make, such as a member added, makes the roots again; an application
    that makes a root of its files for a window it opens beside the first
    asks a reload to make that one again too (`Engine::remake_on_reload`),
    so no window is left on what was replaced.

    Scenario: A save no patch can make is made again in every window
      Given the user's file "Card.qml":
        """qml
        import QtQuick
        Rectangle { property string by: "the user"; width: 30; height: 10 }
        """
      When the application runs "Main.qml" built ahead, with the user's directory over it
      And the file "Second.qml" is opened in a window of its own
      And the user saves the file "Card.qml" as:
        """qml
        import QtQuick
        Rectangle { signal remade; property string by: "the user, again"; width: 30; height: 10 }
        """
      Then the property "by" of "card" is "the user, again"
      And the property "by" of "second-card" is "the user, again"
      And there is no reload error
      And the application was told of no fault

    Scenario: A user's file removed gives way to the application's in every window
      Given the user's file "Card.qml":
        """qml
        import QtQuick
        Rectangle { property string by: "the user"; width: 30; height: 10 }
        """
      When the application runs "Main.qml" built ahead, with the user's directory over it
      And the file "Second.qml" is opened in a window of its own
      Then the property "by" of "second-card" is "the user"
      When the user removes the file "Card.qml"
      Then the property "by" of "card" is "the application"
      And the property "by" of "second-card" is "the application"
      And the property "width" of "second-card" is 10
      And there is no reload error
      And the application was told of no fault

    Scenario: A save is applied in every window with what each holds kept
      Given the user's file "Card.qml":
        """qml
        import QtQuick
        Rectangle { property string by: "the user"; width: 30; height: 10 }
        """
      When the application runs "Main.qml" built ahead, with the user's directory over it
      And the file "Second.qml" is opened in a window of its own
      And the user saves the file "Card.qml" as:
        """qml
        import QtQuick
        Rectangle { property string by: "the user, again"; width: 30; height: 10 }
        """
      Then the property "by" of "card" is "the user, again"
      And the property "by" of "second-card" is "the user, again"
      And 1 save of the user's files has been applied
      And there is no reload error

  Rule: An application whose users keep their files in a layout of its own names it

    An application whose users already keep files in a directory of their
    own, laid out its way, says how: a directory in it whose files have the
    names they would have on the application's import path (its modules),
    and a `theme.json` that is the application's to read, not a theme of
    Ljós.

    Scenario: A user's file in the directory of modules replaces the application's of that name
      Given the application keeps its user's modules in "qml"
      And the user's file "qml/Card.qml":
        """qml
        import QtQuick
        Rectangle { property string by: "the user's module"; width: 30; height: 10 }
        """
      And the user's file "Shell.qml":
        """qml
        import QtQuick
        Rectangle { objectName: "user-shell"; width: 50; height: 20 }
        """
      When the application runs "Main.qml" built ahead, with the user's directory over it
      Then the user's files are compiled
      And the property "by" of "card" is "the user's module"
      And the object "loader/user-shell" exists
      And the application was told of no fault

    Scenario: A module in the user's directory of modules is the one its importers find
      Given the file "Main.qml":
        """qml
        import QtQuick
        import Parts
        Item {
          objectName: "main"
          Badge { objectName: "badge" }
        }
        """
      And the file "Parts/qmldir":
        """
        module Parts
        Badge 1.0 Badge.qml
        """
      And the file "Parts/Badge.qml":
        """qml
        import QtQuick
        Rectangle { property string by: "the application"; width: 10; height: 10 }
        """
      And the QML:
        """qml
        import QtQuick
        Item { objectName: "root" }
        """
      And the application keeps its user's modules in "qml"
      And the user's file "qml/Parts/Badge.qml":
        """qml
        import QtQuick
        Rectangle { property string by: "the user's module"; width: 10; height: 10 }
        """
      When the application runs "Main.qml" built ahead, with the user's directory over it
      Then the property "by" of "badge" is "the user's module"
      And the application was told of no fault

    Scenario: A save in the directory of modules is applied with what the user typed kept
      Given the application keeps its user's modules in "qml"
      When the application runs "Main.qml" built ahead, with the user's directory over it
      And the user types "hello"
      And the user saves the file "qml/Card.qml" as:
        """qml
        import QtQuick
        Rectangle { property string by: "the user's module"; width: 30; height: 10 }
        """
      Then the property "by" of "card" is "the user's module"
      And the property "text" of "field" is "hello"
      And there is no reload error
      And the application was told of no fault

    Scenario: A user's file in the directory of modules removed gives way to the application's with what the user typed kept
      Given the application keeps its user's modules in "qml"
      And the user's file "qml/Card.qml":
        """qml
        import QtQuick
        Rectangle { property string by: "the user's module"; width: 30; height: 10 }
        """
      When the application runs "Main.qml" built ahead, with the user's directory over it
      And the user types "hello"
      Then the property "by" of "card" is "the user's module"
      When the user removes the file "qml/Card.qml"
      Then the property "by" of "card" is "the application"
      And the property "text" of "field" is "hello"
      And there is no reload error
      And the application was told of no fault

    Scenario: A module only the user has, declared by its own qmldir, is found by the user's shell
      Given the application keeps its user's modules in "qml"
      And the user's file "qml/Parts/qmldir":
        """
        module Parts
        Badge 1.0 Badge.qml
        """
      And the user's file "qml/Parts/Badge.qml":
        """qml
        import QtQuick
        Rectangle { property string by: "the user's module"; width: 10; height: 10 }
        """
      And the user's file "Shell.qml":
        """qml
        import QtQuick
        import Parts
        Rectangle {
          objectName: "user-shell"
          width: 50; height: 20
          Badge { objectName: "badge" }
        }
        """
      When the application runs "Main.qml" built ahead, with the user's directory over it
      Then the user's files are compiled
      And the object "loader/user-shell" exists
      And the property "by" of "badge" is "the user's module"
      And the application was told of no fault

    Scenario: A user's file beside the directory of modules is hidden by the module's of its name, and back once that is removed
      Given the application keeps its user's modules in "qml"
      And the user's file "Card.qml":
        """qml
        import QtQuick
        Rectangle { property string by: "the user's own"; width: 20; height: 10 }
        """
      And the user's file "qml/Card.qml":
        """qml
        import QtQuick
        Rectangle { property string by: "the user's module"; width: 30; height: 10 }
        """
      When the application runs "Main.qml" built ahead, with the user's directory over it
      And the user types "hello"
      Then the property "by" of "card" is "the user's module"
      When the user removes the file "qml/Card.qml"
      Then the property "by" of "card" is "the user's own"
      And the property "text" of "field" is "hello"
      And there is no reload error
      And the application was told of no fault

    Scenario: A user's file that links out of the directory, hidden by a module's of its name, is not read and not told
      Given the application keeps its user's modules in "qml"
      And the user's file "Card.qml" links to a file outside the user's directory
      And the user's file "qml/Card.qml":
        """qml
        import QtQuick
        Rectangle { property string by: "the user's module"; width: 30; height: 10 }
        """
      When the application runs "Main.qml" built ahead, with the user's directory over it
      Then the property "by" of "card" is "the user's module"
      And the object "outside" does not exist
      And the application was told of no fault

    Scenario: A theme the application reads itself is no theme of Ljós, and nothing is compiled for it
      Given the file "Main.qml":
        """qml
        import QtQuick
        import Ljos.Blocks
        import Ljos.Design
        Item {
          objectName: "main"
          property bool dusk: String(Colors.accent) === "#9c4dc4"
          property bool forest: String(Colors.accent) === "#2e7d32"
          ShellLoader { objectName: "loader"; source: "Shell.qml"; fallback: "DefaultShell.qml" }
        }
        """
      And the application reads its user's "theme.json" itself
      And the user's file "theme.json":
        """json
        { "Colors": { "accent": "#9c4dc4" } }
        """
      When the application runs "Main.qml" built ahead, with the user's directory over it
      Then the user's files are not compiled
      And the property "dusk" of "main" is false
      And the application was told of no fault
      When the user saves the file "theme.json" as:
        """json
        { "Colors": { "accent": "#2e7d32" } }
        """
      Then the user's files are not compiled
      When the user saves the file "Shell.qml" as:
        """qml
        import QtQuick
        Rectangle { objectName: "user-shell"; width: 50; height: 20 }
        """
      Then the user's files are compiled
      And the object "loader/user-shell" exists
      And the property "dusk" of "main" is false
      And the property "forest" of "main" is false
      And the application was told of no fault
