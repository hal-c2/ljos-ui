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
          Behavior on color { ColorAnimation { duration: 100 } }
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
        would lower app/Main.qml (4 edits)
        3 files, 2 lowered, 0 kept
        """
      When the author runs "ljos render app --out kept.png --settle 0 --compat keep"
      And the author runs "ljos render app --out lowered.png --settle 0 --compat lower"
      Then the pictures "kept.png" and "lowered.png" have the same pixels

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
