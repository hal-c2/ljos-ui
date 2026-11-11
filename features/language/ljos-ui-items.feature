# Sources:
#   docs/DIALECT.md §4.3 (Screen.points), §4.14 (the items whose lowering is "none")
#   https://doc.qt.io/qt-6/qml-qtquick-textinput.html
#   https://doc.qt.io/qt-6/qml-qtquick-keys.html
#   https://doc.qt.io/qt-6/qml-qtquick-shortcut.html
#   https://doc.qt.io/qt-6/qml-qtquick-screen.html
#   https://doc.qt.io/qt-6/qml-qtquick-window.html
# Verified on Qt 6.11.2 (qml6, offscreen).
# `Ljos.Ui` is Ljós's own module. The items here are Qt Quick's as they are, under the same names,
# so each Ljós file below is the Qt file with its import changed, and holds the numbers the Qt file
# holds in the feature that pins the item. `Screen.points` is Ljós's own: Qt has a `pointSize` in
# the place of it.

Feature: The items Ljós's API has as Qt Quick has them
  An image, a flickable, a text field, its keys, its shortcuts, what it
  says to assistive technology, and the window and screen it is shown on
  say the one thing they are in Qt Quick, so Ljós's API has them under
  the same names. A file written for Qt Quick that uses them is lowered
  with nothing changed for them.

  Rule: A file in Ljós's API has them

    @ljos-only
    Scenario: A field takes text, its keys and a shortcut, and says its name
      Given the QML:
        """qml
        import Ljos.Ui
        Item {
          id: root
          objectName: "root"
          width: 200; height: 100
          property int escapes: 0
          property int finds: 0
          TextInput {
            objectName: "field"
            width: 200; height: 20
            focus: true
            Accessible.name: "Search"
            Keys.onEscapePressed: root.escapes += 1
          }
          Shortcut { sequence: "Ctrl+F"; onActivated: root.finds += 1 }
        }
        """
      When the user types "abc"
      Then the property "text" of "field" is "abc"
      When the user presses "Escape"
      Then the property "escapes" of "root" is 1
      When the user presses "Ctrl+F"
      Then the property "finds" of "root" is 1
      And the accessibility tree has a "textinput" named "Search"

    @ljos-only
    Scenario: A flickable of an image, an editor, metrics and a drop area
      Given the QML:
        """qml
        import Ljos.Ui
        Flickable {
          objectName: "list"
          width: 100; height: 100
          contentHeight: 300
          contentY: 40
          Image { objectName: "picture"; width: 20; height: 20; fillMode: Image.PreserveAspectFit }
          TextEdit { id: editor; objectName: "editor"; y: 30; text: "one\ntwo" }
          TextMetrics { id: metrics; objectName: "metrics"; font: editor.font; text: "one" }
          FontMetrics { id: fontMetrics; objectName: "fontMetrics"; font: editor.font }
          DropArea { objectName: "drop"; width: 100; height: 100 }
          property bool measured: metrics.width > 0 && fontMetrics.height > 0
        }
        """
      Then the property "contentY" of "list" is 40
      And the property "fillMode" of "picture" is 1
      And the property "lineCount" of "editor" is 2
      And the property "measured" of "list" is true
      And the property "containsDrag" of "drop" is false

    @ljos-only
    Scenario: A window of Ljós's API shows its colour
      Given the QML:
        """qml
        import Ljos.Ui
        Window {
          objectName: "window"
          width: 120; height: 80
          visible: true
          color: "#336699"
        }
        """
      Then the pixel at 60, 40 of the window is "#336699"

  Rule: Screen.points gives the pixels of a size in points on the item's screen

    # Points to pixels is the screen's logical dots per inch over 72. The screen of the tests has
    # 96 to the inch (features/window/screen-attached.feature), so 12 points are 16 pixels. The type
    # table says the method gives a number, so a TypeScript file binds it to a `real` as it is.
    @ljos-only
    Scenario: Twelve points are sixteen pixels on a screen of 96 to the inch
      Given the QML:
        """qml
        import Ljos.Ui
        Item {
          objectName: "root"
          readonly property real twelve: Screen.points(12)
          readonly property real half: Screen.points(1.5)
        }
        """
      Then the property "twelve" of "root" is 16
      And the property "half" of "root" is 2

  Rule: A file that uses them is lowered with nothing kept for them

    @ljos-only @headless
    Scenario: A Qt Quick file of these items would be lowered whole
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Window
        Window {
          width: 160; height: 100; visible: true; color: "#202020"
          Flickable {
            anchors.fill: parent
            contentHeight: 200
            Image { width: 16; height: 16; fillMode: Image.PreserveAspectFit }
            Rectangle { y: 20; width: 80; height: 20; color: "#c03030" }
            TextInput {
              y: 50; width: 80; text: "abc"; color: "white"
              Accessible.name: "Name"
              Keys.onReturnPressed: accepted()
            }
            TextEdit { y: 70; text: "one" }
            TextMetrics { text: "abc" }
            FontMetrics { }
            DropArea { anchors.fill: parent }
          }
          Shortcut { sequence: "Ctrl+Q"; onActivated: close() }
          readonly property real wide: Screen.width
        }
        """
      When the author runs "ljos migrate ."
      Then the command prints:
        """
        would lower ./Main.qml (3 edits)
        1 files, 1 lowered, 0 kept
        """
      When the author runs "ljos render . --out kept.png --settle 0 --compat keep"
      And the author runs "ljos render . --out lowered.png --settle 0 --compat lower"
      Then the pictures "kept.png" and "lowered.png" have the same pixels

    @ljos-only @headless
    Scenario: The lowered file is the Qt file with its imports changed
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Window
        Window {
          width: 160; height: 100
          Image { fillMode: Image.Stretch }
          Keys.onPressed: (event) => event.accepted = true
        }
        """
      When the author runs "ljos migrate --write ."
      Then the file "Main.qml" is:
        """qml
        import Ljos.Ui
        Window {
          width: 160; height: 100
          Image { fillMode: Image.Stretch }
          Keys.onPressed: (event) => event.accepted = true
        }
        """
