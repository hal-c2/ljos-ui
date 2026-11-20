# Sources:
#   docs/ARCHITECTURE.md (section 12.1, where the compiler links: a shipped application its users change)
#   docs/site/start/your-own-shell.md (what `user-qml` brings, and what stays `dev-qml`)
#   features/hot-reload/user-qml.feature (the user's files over the application's)
#   features/components/create-qml-object.feature (Qt.createQmlObject as Qt has it)
#   features/components/create-component.feature (Qt.createComponent as Qt has it)
# A shipped application that compiles on the device is a Ljós extension: Qt
# compiles every program it runs, so it has nothing to compare it with.

@ljos-only
Feature: A shipped application its users change makes QML of text and of files it names
  An application built ahead of time that lets its users change it
  (`user-qml`) carries the compiler for their files. The same compiler
  makes an object of QML a script holds as text (`Qt.createQmlObject`),
  and a component of a file the application loads from a directory it
  names (`UserQml::loads_from`) while it runs, as a plugin is. A text
  sees the application's types, and a file the types of the modules it
  imports, with the user's files over them. Nothing is
  read or compiled for it until a script first asks. A text or a file that
  does not compile is an error for the script that asked and is told to
  the application, as a user's file that does not compile is. An
  application built without `user-qml` has no compiler, and says so.

  Background:
    Given the file "Main.qml":
      """qml
      import QtQuick
      Item {
        id: main
        objectName: "main"
        property var kid: null
        property string caught: ""
        property bool notMade: caught.startsWith("Error: Qt.createQmlObject(): failed to create object: ")
        property bool atLineTwo: /Plugin\.qml:2:\d+: /.test(caught)
        property bool noCompiler: caught.endsWith("/Plugin.qml: this program runs compiled QML and has no compiler for a file it was not built with")
        function attempt(f) {
          caught = ""
          try { kid = f() } catch (e) { caught = e.name + ": " + e.message }
        }
        function make(text) { attempt(() => Qt.createQmlObject(text, main)) }
        function makeAt(text, where) { attempt(() => Qt.createQmlObject(text, main, where)) }
        function load(url) {
          caught = ""
          const component = Qt.createComponent(url)
          if (component.status !== Component.Ready) { caught = component.errorString().trim(); return }
          kid = component.createObject(main, { width: 7 })
        }
        Card { objectName: "card" }
      }
      """
    And the file "Card.qml":
      """qml
      import QtQuick
      Rectangle { property string by: "the application"; width: 10; height: 10 }
      """
    And the file "Bricks/qmldir":
      """
      module Bricks
      Brick 1.0 Brick.qml
      """
    And the file "Bricks/Brick.qml":
      """qml
      import QtQuick
      Rectangle { property string by: "the application's module"; width: 10; height: 10 }
      """
    And the QML:
      """qml
      import QtQuick
      Item { objectName: "root" }
      """

  Rule: A text is made into an object on the device

    Scenario: A text is made against the application's types
      When the application runs "Main.qml" built ahead, with the user's directory over it
      And the method "make" of "main" is called with "import QtQuick; Card { objectName: 'made' }"
      Then the property "caught" of "main" is ""
      And the property "by" of "main/made" is "the application"

    Scenario: A text sees the user's file over the application's
      Given the user's file "Card.qml":
        """qml
        import QtQuick
        Rectangle { property string by: "the user"; width: 30; height: 10 }
        """
      When the application runs "Main.qml" built ahead, with the user's directory over it
      And the method "make" of "main" is called with "import QtQuick; Card { objectName: 'made' }"
      Then the property "by" of "main/made" is "the user"
      And the property "width" of "main/made" is 30

    Scenario: A text made after the user's first save sees what was saved
      When the application runs "Main.qml" built ahead, with the user's directory over it
      And the method "make" of "main" is called with "import QtQuick; Card { objectName: 'before' }"
      And the user saves the file "Card.qml" as:
        """qml
        import QtQuick
        Rectangle { property string by: "the user"; width: 30; height: 10 }
        """
      And the method "make" of "main" is called with "import QtQuick; Card { objectName: 'after' }"
      Then the property "by" of "main/after" is "the user"

  Rule: Nothing is read or compiled for a text until a script asks

    Scenario: An application whose scripts make no object of text makes no compiler for one
      When the application runs "Main.qml" built ahead, with the user's directory over it
      Then no compiler was made for text
      And the user's files are not compiled
      And the application was told of no fault

    Scenario: The first text makes the compiler, and leaves the user's files as they were
      When the application runs "Main.qml" built ahead, with the user's directory over it
      And the method "make" of "main" is called with "import QtQuick; Item { objectName: 'made' }"
      Then a compiler was made for text
      And the object "main/made" exists
      And the user's files are not compiled
      And the user's directory is watched

  Rule: A text that does not compile is an error for the script, and is told

    Scenario: A text that does not compile is thrown to the script and told to the application
      When the application runs "Main.qml" built ahead, with the user's directory over it
      And the method "makeAt" of "main" is called with "import QtQuick\nItem { nothing: 3 }", "dyn/bad"
      Then the property "notMade" of "main" is true
      And the application was told of 1 fault matching "dyn/bad"
      And the property "by" of "card" is "the application"

  Rule: A file the application loads from a directory it names is compiled when it is asked for

    Scenario: A component of a file in the application's plugin directory is made, with what it is given
      Given the application's plugin file "Plugin.qml":
        """qml
        import Bricks
        Brick { objectName: "plugin" }
        """
      When the application runs "Main.qml" built ahead, with the user's directory over it
      And the application's plugin "Plugin.qml" is given to the method "load" of "main"
      Then the property "caught" of "main" is ""
      And the property "by" of "main/plugin" is "the application's module"
      And the property "width" of "main/plugin" is 7
      And the application was told of no fault

    Scenario: A plugin of two files sees the other file beside it
      Given the application's plugin file "Helper.qml":
        """qml
        import QtQuick
        Rectangle { property string by: "the plugin's helper"; width: 10; height: 10 }
        """
      And the application's plugin file "Plugin.qml":
        """qml
        Helper { objectName: "plugin" }
        """
      When the application runs "Main.qml" built ahead, with the user's directory over it
      And the application's plugin "Plugin.qml" is given to the method "load" of "main"
      Then the property "caught" of "main" is ""
      And the property "by" of "main/plugin" is "the plugin's helper"
      And the application was told of no fault

    Scenario: A file in neither the plugin directory nor the user's is refused and told
      Given the file "Elsewhere.qml" outside the application's directories:
        """qml
        import QtQuick
        Item { objectName: "elsewhere" }
        """
      When the application runs "Main.qml" built ahead, with the user's directory over it
      And the file "Elsewhere.qml" outside the application's directories is given to the method "load" of "main"
      Then the object "main/elsewhere" does not exist
      And the application was told of a fault matching "refused: .*Elsewhere\.qml"

    Scenario: A file of the plugin directory that links out of it is refused and told
      Given the file "Elsewhere.qml" outside the application's directories:
        """qml
        import QtQuick
        Item { objectName: "elsewhere" }
        """
      And the application's plugin file "Plugin.qml" links to the file "Elsewhere.qml" outside the application's directories
      When the application runs "Main.qml" built ahead, with the user's directory over it
      And the application's plugin "Plugin.qml" is given to the method "load" of "main"
      Then the object "main/elsewhere" does not exist
      And the application was told of a fault matching "Plugin\.qml: a link that leads out"

    Scenario: A file in the application's plugin directory sees the user's file over the module's
      Given the user's file "Bricks/Brick.qml":
        """qml
        import QtQuick
        Rectangle { property string by: "the user"; width: 30; height: 10 }
        """
      And the application's plugin file "Plugin.qml":
        """qml
        import Bricks
        Brick { objectName: "plugin" }
        """
      When the application runs "Main.qml" built ahead, with the user's directory over it
      And the application's plugin "Plugin.qml" is given to the method "load" of "main"
      Then the property "caught" of "main" is ""
      And the property "by" of "main/plugin" is "the user"

    Scenario: A file of the plugin directory fetched again is read once the user's files change
      Given the user's file "Card.qml":
        """qml
        import QtQuick
        Rectangle { property string by: "the user"; width: 30; height: 10 }
        """
      And the application's plugin file "Plugin.qml":
        """qml
        import QtQuick
        Item { objectName: "first"; property string by: "the first fetch" }
        """
      When the application runs "Main.qml" built ahead, with the user's directory over it
      And the application's plugin "Plugin.qml" is given to the method "load" of "main"
      And the application fetches its plugin file "Plugin.qml" again:
        """qml
        import QtQuick
        Item { objectName: "second"; property string by: "the second fetch" }
        """
      And the application's plugin "Plugin.qml" is given to the method "load" of "main"
      Then the property "by" of "main/first" is "the first fetch"
      And the object "main/second" does not exist
      When the user saves the file "Card.qml" as:
        """qml
        import QtQuick
        Rectangle { property string by: "the user, again"; width: 30; height: 10 }
        """
      And the application's plugin "Plugin.qml" is given to the method "load" of "main"
      Then the property "caught" of "main" is ""
      And the property "by" of "main/second" is "the second fetch"
      And the application was told of no fault

    Scenario: A text placed in the application's plugin directory is made, not refused
      When the application runs "Main.qml" built ahead, with the user's directory over it
      And the text "import Bricks; Brick { objectName: 'registered' }" is given to the method "makeAt" of "main" with the address of the application's plugin "Registry.qml"
      Then the property "caught" of "main" is ""
      And the property "by" of "main/registered" is "the application's module"
      And the application was told of no fault

    Scenario: A file in the application's plugin directory that does not compile is the script's error, and is told
      Given the application's plugin file "Plugin.qml":
        """qml
        import QtQuick
        Item { width: }
        """
      When the application runs "Main.qml" built ahead, with the user's directory over it
      And the application's plugin "Plugin.qml" is given to the method "load" of "main"
      Then the property "atLineTwo" of "main" is true
      And the application was told of 1 fault matching "Plugin\.qml"

  Rule: An application built without its users' compiler says it has none

    Scenario: A text is an error that says the program has no compiler
      When the application runs "Main.qml" built ahead, without a user's directory
      And the method "make" of "main" is called with "import QtQuick; Card { objectName: 'made' }"
      Then the property "caught" of "main" is "Error: Qt.createQmlObject(): this program runs compiled QML and has no compiler to make an object from text"
      And the object "main/made" does not exist

    Scenario: A file it was not built with is an error that says the program has no compiler
      Given the application's plugin file "Plugin.qml":
        """qml
        import Bricks
        Brick { objectName: "plugin" }
        """
      When the application runs "Main.qml" built ahead, without a user's directory
      And the application's plugin "Plugin.qml" is given to the method "load" of "main"
      Then the property "noCompiler" of "main" is true
      And the object "main/plugin" does not exist
