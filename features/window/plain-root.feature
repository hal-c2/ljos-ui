# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-window.html#attached-properties
#   https://doc.qt.io/qt-6/qml-qtquick-window.html#visibility-prop
# Verified on Qt 6.11.2 (qmltestrunner, offscreen).

@qt-parity
Feature: The window of a root that is not a Window
  An item whose root is no Window is still shown in a window. `Window.window`
  names that window, which an application can go full screen with like any
  other, and which is not the root.

  Rule: The window is not the root

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Window
        Item {
          id: root
          objectName: "root"
          width: 300; height: 200
          readonly property var win: Window.window
          readonly property bool isRoot: win === root
          readonly property bool hasContent: win.contentItem !== null && win.contentItem !== undefined
          readonly property bool sameContent: Window.contentItem === win.contentItem
          readonly property bool sameForKid: kid.Window.window === win
          readonly property int attached: Window.visibility
          readonly property int own: win.visibility
          readonly property bool shown: win.visible
          readonly property bool active: win.active
          readonly property string title: win.title
          property bool focusSame: false
          function nameIt() { win.title = "named" }
          function checkFocus() { kid.forceActiveFocus(); focusSame = win.activeFocusItem === kid }
          Item { id: kid; objectName: "kid" }
        }
        """

    Scenario: It is a window with what a window has
      Then the property "isRoot" of "root" is false
      And the property "hasContent" of "root" is true
      And the property "sameContent" of "root" is true

    Scenario: Every item of the tree has the same one
      Then the property "sameForKid" of "root" is true

    Scenario: It says what the attached properties say
      Then the property "own" of "root" is 2
      And the property "attached" of "root" is 2
      And the property "shown" of "root" is true
      And the property "active" of "root" is true

    Scenario: It knows the item with focus
      When the method "checkFocus" of "root" is called
      Then the property "focusSame" of "root" is true

    Scenario: Its title can be written
      When the method "nameIt" of "root" is called
      Then the property "title" of "root" is "named"

  Rule: It can go full screen and back

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Window
        Item {
          id: root
          objectName: "root"
          width: 300; height: 200
          readonly property var win: Window.window
          readonly property int attached: Window.visibility
          readonly property int own: win.visibility
          readonly property bool shown: win.visible
          property int kinds: 0
          function ask() {
            kinds = ["show", "hide", "showNormal", "showMinimized", "showMaximized", "showFullScreen",
                     "close", "raise", "lower", "requestActivate", "alert", "startSystemMove"]
                    .filter(k => typeof win[k] === "function").length
          }
          function fullScreen() { win.visibility = Window.FullScreen }
          function windowed() { win.visibility = Window.Windowed }
          function showFull() { win.showFullScreen() }
          function showBack() { win.showNormal() }
        }
        """

    Scenario: It has a window's methods
      When the method "ask" of "root" is called
      Then the property "kinds" of "root" is 12

    Scenario: By its visibility
      When the method "fullScreen" of "root" is called
      Then the property "own" of "root" is 5
      And the property "attached" of "root" is 5
      When the method "windowed" of "root" is called
      Then the property "own" of "root" is 2
      And the property "attached" of "root" is 2

    Scenario: By its methods
      When the method "showFull" of "root" is called
      Then the property "own" of "root" is 5
      And the property "attached" of "root" is 5
      When the method "showBack" of "root" is called
      Then the property "own" of "root" is 2
      And the property "attached" of "root" is 2
      And the property "shown" of "root" is true
