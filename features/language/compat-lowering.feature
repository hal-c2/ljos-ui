# Sources:
#   docs/DIALECT.md (Ljós's own API, the compat pass)
#   docs/adr/0004-compat-lowering-before-the-ir.md
#   https://doc.qt.io/qt-6/qml-qtquick-rectangle.html
#   https://doc.qt.io/qt-6/qml-qtquick-mousearea.html
#   https://doc.qt.io/qt-6/qml-qtquick-propertychanges.html
# Verified on Qt 6.11.2 (qml6, offscreen).
# `Ljos.Ui` is Ljós's own module, and lowering a file to it is Ljós's: the scenarios that lower are
# `@ljos-only`. The button as written is Qt's QML and runs on both, so it is what the lowered one is
# held to: the same states, the same handlers and the same pixels. Its colour changes three times:
# once as the first state is entered after the binding, and once for each half of the click.

Feature: Qt's QML lowered to Ljós's API
  Ljós's own API renames what Qt's names for one thing of several:
  a Rectangle's `color` is its `fill`, and a MouseArea, which takes a
  pen and a finger too, is a PointerArea. A file written for Qt Quick
  is lowered to it as it loads, every name at once, and makes the same
  objects; a file the pass cannot lower whole runs as it is written.

  Background:
    Given the file "Button.qml":
      """qml
      import QtQuick
      Item {
        id: button
        property color tint: "#2060c0"
        property int clicks: 0
        property int repaints: 0
        state: "NORMAL"
        Rectangle {
          id: face; objectName: "face"; anchors.fill: parent; color: button.tint
          onColorChanged: button.repaints += 1
        }
        MouseArea {
          anchors.fill: parent
          onPressed: button.state = "PRESSED"
          onReleased: button.state = "NORMAL"
          onClicked: button.clicks += 1
        }
        states: [
          State { name: "NORMAL"; PropertyChanges { target: face; color: button.tint } },
          State { name: "PRESSED"; PropertyChanges { target: face; color: "#ffffff" } }
        ]
      }
      """

  Rule: A file lowered whole does what it did as written

    @qt-parity
    Scenario: The button as written takes a click
      Given the QML:
        """qml
        import QtQuick
        Item { width: 100; height: 60; Button { objectName: "button"; anchors.fill: parent } }
        """
      When the user presses the pointer at 50, 30
      Then the property "state" of "button" is "PRESSED"
      And the pixel at 50, 30 of the window is "#ffffff"
      When the user releases the pointer at 50, 30
      Then the property "state" of "button" is "NORMAL"
      And the property "clicks" of "button" is 1
      And the property "repaints" of "button" is 3
      And the pixel at 50, 30 of the window is "#2060c0"

    @ljos-only
    Scenario: The button lowered takes the same click
      Given Qt QML is lowered to the Ljós API
      And the QML:
        """qml
        import QtQuick
        Item { width: 100; height: 60; Button { objectName: "button"; anchors.fill: parent } }
        """
      Then the property "fill" of "face" is "#2060c0"
      When the user presses the pointer at 50, 30
      Then the property "state" of "button" is "PRESSED"
      And the pixel at 50, 30 of the window is "#ffffff"
      When the user releases the pointer at 50, 30
      Then the property "state" of "button" is "NORMAL"
      And the property "clicks" of "button" is 1
      And the property "repaints" of "button" is 3
      And the pixel at 50, 30 of the window is "#2060c0"

  Rule: A file the pass cannot lower whole is run as it is written

    @ljos-only
    Scenario: A type Ljós's API has not taken yet keeps its file as written
      Given Qt QML is lowered to the Ljós API
      And the QML:
        """qml
        import QtQuick
        Rectangle {
          objectName: "swatch"; width: 40; height: 40; color: "#336699"
          transitions: Transition { AnchorAnimation { duration: 100 } }
        }
        """
      Then the property "color" of "swatch" is "#336699"
      And the pixel at 20, 20 of the window is "#336699"

  Rule: `ljos run --compat lower` runs an application lowered, to be driven

    # As `ljos render --compat lower` renders it: the files are lowered as they
    # load, and the program the lowered files make is the one that runs.
    @ljos-only @headless
    Scenario: A lowered application runs and takes a click
      Given the file "Main.qml":
        """qml
        import QtQuick
        Item { width: 100; height: 60; Button { objectName: "button"; anchors.fill: parent } }
        """
      When the author starts "ljos run Main.qml --headless --compat lower --mcp=inspect,input,eval"
      And the agent calls the tool "app_eval" with:
        """json
        {"target": "#face", "source": "String(fill)"}
        """
      Then the answer's "/value" is "#2060c0"
      When the agent calls the tool "app_click" with:
        """json
        {"target": "#button"}
        """
      And the agent calls the tool "app_eval" with:
        """json
        {"target": "#button", "source": "clicks"}
        """
      Then the answer's "/value" is 1

    @ljos-only @headless
    Scenario: An application run as written keeps Qt's names
      Given the file "Main.qml":
        """qml
        import QtQuick
        Item { width: 100; height: 60; Button { objectName: "button"; anchors.fill: parent } }
        """
      When the author starts "ljos run Main.qml --headless --compat keep --mcp=inspect,eval"
      And the agent calls the tool "app_eval" with:
        """json
        {"target": "#face", "source": "String(color)"}
        """
      Then the answer's "/value" is "#2060c0"
  Rule: A module of the project is lowered with the files that import it

    # A module's types are files of the project, as the loader finds them on its import path: the
    # pass reads what each one's root is, as it does for a file beside the one it lowers, and the
    # module is lowered with its importers or kept with them.
    @ljos-only @headless
    Scenario: An application and the module of components it imports are lowered whole
      Given the file "app/Gauges/qmldir":
        """
        module Gauges
        Dial 1.0 Dial.qml
        singleton Tones 1.0 Tones.qml
        """
      And the file "app/Gauges/Tones.qml":
        """qml
        pragma Singleton
        import QtQml
        QtObject { property color color: "#30a050" }
        """
      And the file "app/Gauges/Dial.qml":
        """qml
        import QtQuick
        Rectangle {
          width: 60; height: 60; radius: 30; color: "#203040"
          Rectangle { anchors.centerIn: parent; width: 20; height: 20; color: Tones.color }
        }
        """
      And the file "app/Main.qml":
        """qml
        import QtQuick
        import Gauges
        Rectangle {
          width: 200; height: 100; color: "white"
          Dial { x: 20; y: 20; color: "#c03030" }
          Dial { x: 120; y: 20; color: Tones.color }
        }
        """
      When the author runs "ljos migrate app"
      Then the command prints:
        """
        would lower app/Gauges/Dial.qml (3 edits)
        would lower app/Gauges/Tones.qml (2 edits)
        would lower app/Main.qml (4 edits)
        3 files, 3 lowered, 0 kept
        """
      When the author runs "ljos render app --out kept.png --settle 0 --compat keep"
      And the author runs "ljos render app --out lowered.png --settle 0 --compat lower"
      Then the pictures "kept.png" and "lowered.png" have the same pixels

    # A file named alone is lowered against the files around it. One it uses that was not named stays
    # Qt's, and the named file keeps Qt's names on that one's type, so it is written: a Ljós file reads
    # a kept type's names as its file declares them. That one, named later, stays Qt's while a file in
    # Ljós's API binds on it a name its lowering renames, and the command says which and to what.
    @ljos-only @headless
    Scenario: A file named alone is lowered against a file it uses that stays Qt's
      Given the file "app/Gauges/qmldir":
        """
        module Gauges
        Dial 1.0 Dial.qml
        """
      And the file "app/Gauges/Dial.qml":
        """qml
        import QtQuick
        Rectangle { width: 60; height: 60; color: "#203040" }
        """
      And the file "app/Main.qml":
        """qml
        import QtQuick
        import Gauges
        Rectangle { width: 200; height: 100; Dial { color: "#c03030" } }
        """
      When the author runs "ljos render app --out kept.png --settle 0 --compat keep"
      And the author runs "ljos migrate --write app/Main.qml"
      Then the command prints:
        """
        lowered app/Main.qml (1 edits)
        1 files, 1 lowered, 0 kept
        """
      And the file "app/Main.qml" is:
        """qml
        import Ljos.Ui
        import Gauges
        Rectangle { width: 200; height: 100; Dial { color: "#c03030" } }
        """
      When the author runs "ljos check app"
      Then the command exits with status 0
      When the author runs "ljos render app --out migrated.png --settle 0 --compat keep"
      Then the pictures "kept.png" and "migrated.png" have the same pixels
      When the author runs "ljos migrate --write app/Gauges/Dial.qml"
      Then the command prints:
        """
        kept app/Gauges/Dial.qml:1:1: `Main.qml`, in Ljós's API, binds `color` on it, which Ljós's `Rectangle` calls `fill`: rename it there as this file is lowered
        1 files, 0 lowered, 1 kept
        """
      And the file "app/Gauges/Dial.qml" is:
        """qml
        import QtQuick
        Rectangle { width: 60; height: 60; color: "#203040" }
        """

    # A file in Ljós's API that binds on a Qt file's type only names Ljós calls as Qt does holds nothing
    # back: that file is lowered under it.
    @ljos-only @headless
    Scenario: A file in Ljós's API lets the Qt file it uses be lowered
      Given the file "app/Gauges/qmldir":
        """
        module Gauges
        Dial 1.0 Dial.qml
        """
      And the file "app/Gauges/Dial.qml":
        """qml
        import QtQuick
        Rectangle { width: 60; height: 60; color: "#203040" }
        """
      And the file "app/Main.qml":
        """qml
        import Ljos.Ui
        import Gauges
        Rectangle { width: 200; height: 100; Dial { width: 80 } }
        """
      When the author runs "ljos render app --out kept.png --settle 0 --compat keep"
      And the author runs "ljos migrate --write app/Gauges/Dial.qml"
      Then the command prints:
        """
        lowered app/Gauges/Dial.qml (2 edits)
        1 files, 1 lowered, 0 kept
        """
      And the file "app/Gauges/Dial.qml" is:
        """qml
        import Ljos.Ui
        Rectangle { width: 60; height: 60; fill: "#203040" }
        """
      When the author runs "ljos check app"
      Then the command exits with status 0
      When the author runs "ljos render app --out migrated.png --settle 0 --compat keep"
      Then the pictures "kept.png" and "migrated.png" have the same pixels

    # A file the pass cannot lower keeps the files it uses, and only those: the application that uses
    # it is lowered around it, so an application is migrated a file at a time.
    @ljos-only @headless
    Scenario: A kept component keeps the files it uses, not the application that uses it
      Given the file "app/Gauges/qmldir":
        """
        module Gauges
        Dial 1.0 Dial.qml
        Needle 1.0 Needle.qml
        """
      And the file "app/Gauges/Needle.qml":
        """qml
        import QtQuick
        Rectangle { width: 4; height: 24; color: "#e0e0e0" }
        """
      And the file "app/Gauges/Dial.qml":
        """qml
        import QtQuick
        Rectangle {
          width: 60; height: 60; color: "#203040"
          function copy() { return Qt.createQmlObject("import QtQuick; Item {}", this) }
          Needle { x: 28; y: 6 }
        }
        """
      And the file "app/Main.qml":
        """qml
        import QtQuick
        import Gauges
        Rectangle {
          width: 200; height: 100; color: "white"
          Dial { x: 20; y: 20; color: "#c03030" }
          Rectangle { x: 120; y: 20; width: 60; height: 60; color: "#30a050" }
        }
        """
      When the author runs "ljos migrate app"
      Then the command prints:
        """
        kept app/Gauges/Dial.qml:4:31: it makes objects from QML text, which is not lowered yet
        kept app/Gauges/Needle.qml:1:1: it is used by `Dial.qml`, which is kept: it makes objects from QML text, which is not lowered yet
        would lower app/Main.qml (3 edits)
        3 files, 1 lowered, 2 kept
        """
      When the author runs "ljos render app --out kept.png --settle 0 --compat keep"
      And the author runs "ljos render app --out lowered.png --settle 0 --compat lower"
      Then the pictures "kept.png" and "lowered.png" have the same pixels
      When the author runs "ljos migrate --write app"
      And the author runs "ljos check app"
      Then the command exits with status 0
      When the author runs "ljos render app --out migrated.png --settle 0 --compat keep"
      Then the pictures "kept.png" and "migrated.png" have the same pixels

    # A directory imported by its path has no qmldir to say where it is imported from: the file that
    # imports it is found below the directory the command runs in.
    @ljos-only @headless
    Scenario: A file of a directory imported by its path is not written without the file that imports it
      Given the file "app/parts/Dial.qml":
        """qml
        import QtQuick
        Rectangle { width: 60; height: 60; color: "#203040" }
        """
      And the file "app/Main.qml":
        """qml
        import QtQuick
        import "parts"
        Rectangle { width: 200; height: 100; Dial { color: "#c03030" } }
        """
      When the author runs "ljos migrate --write app/parts/Dial.qml"
      Then the command prints:
        """
        kept app/parts/Dial.qml:1:1: it is used by `Main.qml`, which is kept: it is not named: name it too
        1 files, 0 lowered, 1 kept
        """
      And the file "app/parts/Dial.qml" is:
        """qml
        import QtQuick
        Rectangle { width: 60; height: 60; color: "#203040" }
        """

    @ljos-only @headless
    Scenario: A module with native types only keeps the files that import it
      Given the file "app/Sensors/qmldir":
        """
        module Sensors
        plugin sensorsplugin
        """
      And the file "app/Main.qml":
        """qml
        import QtQuick
        import Sensors
        Rectangle { width: 40; height: 40; color: "#336699" }
        """
      When the author runs "ljos migrate app"
      Then the command prints:
        """
        kept app/Main.qml:2:1: it imports `Sensors`, a module of the project whose types are native: they are not lowered
        1 files, 0 lowered, 1 kept
        """

  Rule: Every Qt import is lowered, and none is left as Qt's

    # A Qt module imported twice is one Ljós module's line, the last import's: the other import's
    # line is emptied, as a module whose types are another's is, so the lowered file has no import
    # of Qt's that a file of Ljós's API could not load.
    @ljos-only @headless
    Scenario: A module imported twice is imported once lowered
      Given the file "twice/Main.qml":
        """qml
        import QtQuick
        import QtQuick.Layouts
        import QtQuick.Layouts
        Rectangle {
          width: 80; height: 40; color: "#202020"
          RowLayout {
            Rectangle { implicitWidth: 20; implicitHeight: 20; color: "#c03030" }
            Rectangle { implicitWidth: 20; implicitHeight: 20; color: "#30a050" }
          }
        }
        """
      When the author runs "ljos render twice --out kept.png --settle 0 --compat keep"
      And the author runs "ljos render twice --out lowered.png --settle 0 --compat lower"
      Then the pictures "kept.png" and "lowered.png" have the same pixels
      When the author runs "ljos migrate --write twice"
      Then the file "twice/Main.qml" is:
        """qml
        import Ljos.Ui
        import Ljos.Layout
        Rectangle {
          width: 80; height: 40; fill: "#202020"
          Row {
            spacing: 5
            Rectangle { Layout.alignment: Align.VCenter; implicitWidth: 20; implicitHeight: 20; fill: "#c03030" }
            Rectangle { Layout.alignment: Align.VCenter; implicitWidth: 20; implicitHeight: 20; fill: "#30a050" }
          }
        }
        """

  Rule: A migrated file is written in Ljós's API, typed where its scripts already are

    # The pass keeps each line where it was, so a lowered run's findings point at the file as
    # written; migrate writes the file a person reads from then on, so its imports are laid out one
    # to a line, a binding the pass writes out after an object's `{` or a list's `]` goes on a line
    # of its own, and a line the pass empties goes. `pragma Scripts: JavaScript` only turns the checker's rules off, and the program is
    # the same without it: migrate leaves it out where the checker finds nothing more without it.
    @ljos-only @headless
    Scenario: A file whose scripts keep the checker's rules is migrated typed
      Given the file "app/Main.qml":
        """qml
        import QtQuick
        import QtQuick.Layouts

        Rectangle {
          width: 120; height: 60; color: "#202020"
          property int taps: 0
          function bump(by: int) { taps += by }
          ColumnLayout {
            Rectangle { implicitWidth: 20; implicitHeight: 20; color: taps === 2 ? "#c03030" : "#30a050" }
          }
        }
        """
      When the author runs "ljos migrate --write app"
      Then the command prints:
        """
        lowered app/Main.qml (6 edits)
        1 files, 1 lowered, 0 kept
        """
      And the file "app/Main.qml" is:
        """qml
        import Ljos.Ui
        import Ljos.Layout

        Rectangle {
          width: 120; height: 60; fill: "#202020"
          property int taps: 0
          function bump(by: int) { taps += by }
          Column {
            spacing: 5
            Rectangle { implicitWidth: 20; implicitHeight: 20; fill: taps === 2 ? "#c03030" : "#30a050" }
          }
        }
        """
      When the author runs "ljos check app"
      Then the command exits with status 0
      When the author runs "ljos migrate --write app"
      Then the command prints:
        """
        1 files, 0 lowered, 0 kept
        """

    @ljos-only @headless
    Scenario: A file whose scripts break the checker's rules keeps its scripts JavaScript
      Given the file "app/Main.qml":
        """qml
        import QtQuick
        Rectangle {
          width: 120; height: 60; color: "#202020"
          property int taps: 0
          function bump(by) { taps += by }
          Rectangle { width: 20; height: 20; color: taps == 2 ? "#c03030" : "#30a050" }
        }
        """
      When the author runs "ljos migrate --write app"
      Then the command prints:
        """
        lowered app/Main.qml (3 edits)
        scripts of app/Main.qml stay JavaScript: typed, ljos check finds 2
        1 files, 1 lowered, 0 kept
        """
      And the file "app/Main.qml" is:
        """qml
        pragma Scripts: JavaScript
        import Ljos.Ui
        Rectangle {
          width: 120; height: 60; fill: "#202020"
          property int taps: 0
          function bump(by) { taps += by }
          Rectangle { width: 20; height: 20; fill: taps == 2 ? "#c03030" : "#30a050" }
        }
        """
      When the author runs "ljos check app"
      Then the command exits with status 0

  Rule: Ljós's own modules are Ljós's API already

    # A file of Ljós's API has nothing to lower. One that imports a module of Ljós's beside Qt's has
    # its Qt imports and what they name lowered, and the import of Ljós's module kept as it is.
    @ljos-only @headless
    Scenario: A file in Ljós's API is left as it is
      Given the file "app/Main.qml":
        """qml
        import Ljos.Ui
        import Ljos.Layout
        Rectangle { fill: "red"; Column { } }
        """
      When the author runs "ljos migrate --write app"
      Then the command prints:
        """
        1 files, 0 lowered, 0 kept
        """

    @ljos-only @headless
    Scenario: A module of Ljós's beside Qt's stays, and the rest is lowered
      Given the file "app/Main.qml":
        """qml
        import QtQuick
        import Ljos.Design
        Rectangle { color: Colors.canvas }
        """
      When the author runs "ljos migrate --write app"
      Then the command prints:
        """
        lowered app/Main.qml (2 edits)
        1 files, 1 lowered, 0 kept
        """
      And the file "app/Main.qml" is:
        """qml
        import Ljos.Ui
        import Ljos.Design
        Rectangle { fill: Colors.canvas }
        """
      When the author runs "ljos check app"
      Then the command exits with status 0

    # `Ljos.Blocks`, what a shell is built of, is a module of Ljós's like `Ljos.Design`: a shell
    # written against QtQuick beside it has its Qt half lowered, and its slots and contributions are
    # carried over as written. Lowered or not, it places the contributions the same way.
    @ljos-only @headless
    Scenario: A shell of blocks beside Qt's is lowered, its slots as they are written
      Given the file "shell/Main.qml":
        """qml
        import QtQuick
        import Ljos.Blocks
        Rectangle {
          width: 60; height: 20; color: "#202020"
          Row { Slot { name: "tools" } }
          Contribution { slot: "tools"; order: 2
            Rectangle { width: 10; height: 10; color: "#c03030" } }
          Contribution { slot: "tools"; order: 1
            Rectangle { width: 10; height: 10; color: "#30a050" } }
        }
        """
      When the author runs "ljos render shell --out kept.png --settle 0 --compat keep"
      And the author runs "ljos render shell --out lowered.png --settle 0 --compat lower"
      Then the pictures "kept.png" and "lowered.png" have the same pixels
      When the author runs "ljos migrate --write shell"
      Then the command prints:
        """
        lowered shell/Main.qml (4 edits)
        1 files, 1 lowered, 0 kept
        """
      And the file "shell/Main.qml" is:
        """qml
        import Ljos.Ui
        import Ljos.Layout
        import Ljos.Blocks
        Rectangle {
          width: 60; height: 20; fill: "#202020"
          Row { Slot { name: "tools" } }
          Contribution { slot: "tools"; order: 2
            Rectangle { width: 10; height: 10; fill: "#c03030" } }
          Contribution { slot: "tools"; order: 1
            Rectangle { width: 10; height: 10; fill: "#30a050" } }
        }
        """
      When the author runs "ljos check shell"
      Then the command exits with status 0

    # The modules the pass writes for Qt's are not carried over beside them: `Ljos.Ui`'s `Rectangle`
    # would stand for QtQuick's, its Qt-named `color` would be left as it is, and the lowered file
    # would import `Ljos.Ui` twice. Such a file is kept, and says which import keeps it.
    @ljos-only @headless
    Scenario: A file that imports Ljos.Ui beside QtQuick is kept
      Given the file "app/Main.qml":
        """qml
        import QtQuick
        import Ljos.Ui
        Rectangle { color: "red" }
        """
      When the author runs "ljos migrate --write app"
      Then the command prints:
        """
        kept app/Main.qml:2:1: it imports `Ljos.Ui` beside Qt's modules, which lower to it
        1 files, 0 lowered, 1 kept
        """
      And the file "app/Main.qml" is:
        """qml
        import QtQuick
        import Ljos.Ui
        Rectangle { color: "red" }
        """

  Rule: A name a script binds is the script's wherever the script reads it

    # A loop's, a `catch`'s or a function's name is found before an id and before the members of the
    # objects around the script, in Qt as once lowered: what is read off it is not the id's, and it is
    # no member a rename would give the object. The pass asks one question for every name a script
    # reads, so a member read off the name and the name itself are told the same.
    @qt-parity
    Scenario: A loop's name is the loop's, not an id or a member of the same name
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          id: swatch; objectName: "swatch"; width: 40; height: 40; color: "#336699"
          property string painted: {
            let c = ""
            for (const swatch of [{ color: "#ffffff" }]) c = swatch.color
            return c
          }
          property int total: { let t = 0; for (const fill of [1, 2]) t += fill; return t }
        }
        """
      Then the property "painted" of "swatch" is "#ffffff"
      And the property "total" of "swatch" is 3

    # What is read off a loop's name is not known, and `color` is a member Ljós renames: the file is
    # kept rather than lowered as if the loop's object were the Rectangle the id names.
    @ljos-only
    Scenario: A member read off a loop's name is not renamed as the id's
      Given Qt QML is lowered to the Ljós API
      And the QML:
        """qml
        import QtQuick
        Rectangle {
          id: swatch; objectName: "swatch"; width: 40; height: 40; color: "#336699"
          property string painted: {
            let c = ""
            for (const swatch of [{ color: "#ffffff" }]) c = swatch.color
            return c
          }
        }
        """
      Then the property "painted" of "swatch" is "#ffffff"

    @ljos-only
    Scenario: A loop's name that a lowered type has as a member keeps no file
      Given Qt QML is lowered to the Ljós API
      And the QML:
        """qml
        import QtQuick
        Rectangle {
          objectName: "swatch"; width: 40; height: 40; color: "#336699"
          property int total: { let t = 0; for (const fill of [1, 2]) t += fill; return t }
        }
        """
      Then the property "fill" of "swatch" is "#336699"
      And the property "total" of "swatch" is 3
