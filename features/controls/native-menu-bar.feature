# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-menubar.html ("Native menu bars")
#   https://doc.qt.io/qt-6/qaction.html#MenuRole-enum
#   https://doc.qt.io/qt-6/qkeysequence.html (Ctrl is Command on macOS, Meta is Control)
# Observed on Qt 6.11.1 (qml, macOS 27, read through the Accessibility
# API): a MenuBar is the system's menu bar there, with no property asking
# for it (`requestNative` does not exist); the bar keeps `visible` true
# with `height` 0, and the window's content starts at its top; item texts
# lose their `&`; Ctrl+Shift+S is ⇧⌘S, Meta+W is ⌃W; an item called
# "About" moves to the application menu, and a menu left empty is not
# shown.
#
# @ljos-only: the oracle runs on Qt's offscreen and vnc platforms, which
# have no menu bar of the system's. "The system" here is the scenario's
# stand-in for one; what macOS makes of it is in ljos_shell.

@ljos-only
Feature: A menu bar the system shows
  Where the system has a menu bar of its own, as macOS has, a MenuBar is
  shown there, with its menus, items, separators and sub-menus. Choosing
  an item there triggers it as choosing it in the window would. Elsewhere
  the bar is in the window.

  Background:
    Given the system shows menu bars of its own
    And the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      ApplicationWindow {
        id: window; objectName: "window"; width: 400; height: 300; visible: true
        property int made: 0
        property int quits: 0
        property real contentTop: contentItem.y
        function allowSave() { saveAs.enabled = true }
        function rename() { make.text = "&Make" }
        function addView() { bar.addMenu(view.createObject(bar)) }
        Component { id: view; Menu { title: "&View"; Action { text: "Zoom" } } }
        menuBar: MenuBar {
          id: bar; objectName: "bar"
          Menu {
            title: "&File"
            Action { id: make; text: "&New"; shortcut: "Ctrl+N"; onTriggered: window.made++ }
            Action { id: saveAs; text: "Save &As…"; shortcut: "Ctrl+Shift+S"; enabled: false }
            MenuSeparator { }
            Menu { title: "Recent"; Action { text: "one.txt" } }
            Action { id: wrap; objectName: "wrap"; text: "Wrap"; checkable: true; checked: true; shortcut: "Meta+W" }
            Action { text: "&Quit"; shortcut: StandardKey.Quit; onTriggered: window.quits++ }
          }
          Menu { title: "&Help"; Action { text: "&About" } }
        }
      }
      """

  @api-QtQuick.Controls.MenuBar
  Scenario: The bar is the system's and takes no room in the window
    Then the property "height" of "bar" is 0
    And the property "visible" of "bar" is true
    And the property "contentTop" of "window" is 0

  @api-QtQuick.Controls.MenuBar @api-QtQuick.Controls.MenuSeparator @api-QtQuick.Controls.Action.shortcut
  Scenario: The system's bar has the menus, items, separators and sub-menus
    Then the system's menu bar is:
      """
      Application
        About
        Quit  ⌘Q
      File
        New  ⌘N
        Save As…  ⇧⌘S  disabled
        ---
        Recent
          one.txt
        ✓ Wrap  ⌃W
      """

  @api-QtQuick.Controls.Action.triggered
  Scenario: Choosing an item in the system's bar triggers it once
    When the user chooses "File > New" in the system's menu bar
    Then the property "made" of "window" is 1

  @api-QtQuick.Controls.Action.checked
  Scenario: Choosing a checkable item turns it over
    When the user chooses "File > Wrap" in the system's menu bar
    Then the property "checked" of "wrap" is false
    And the system's menu bar shows "File > Wrap" unchecked

  @api-QtQuick.Controls.Action.enabled
  Scenario: A disabled item cannot be chosen
    When the user chooses "File > Save As…" in the system's menu bar
    Then the system's menu bar shows "File > Save As…" disabled

  @api-QtQuick.Controls.Action.enabled @api-QtQuick.Controls.Action.text
  Scenario: What changes in the QML changes in the system's bar
    When the method "allowSave" of "window" is called
    And the method "rename" of "window" is called
    And the method "addView" of "window" is called
    Then the system's menu bar shows "File > Save As…" enabled
    And the system's menu bar shows "File > Make"
    And the system's menu bar shows "View > Zoom"

  @api-QtQuick.Controls.Action.triggered
  Scenario: Quit in the application menu triggers the item that quits
    When the user chooses "Quit" in the system's application menu
    Then the property "quits" of "window" is 1
