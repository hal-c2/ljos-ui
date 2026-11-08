# Sources:
#   https://doc.qt.io/qt-6/qml-qt-labs-platform-menubar.html
#   https://doc.qt.io/qt-6/qml-qt-labs-platform-menu.html
#   https://doc.qt.io/qt-6/qml-qt-labs-platform-menuitem.html
#   https://doc.qt.io/qt-6/qml-qt-labs-platform-menuseparator.html
#   https://doc.qt.io/qt-6/qaction.html#MenuRole-enum
# Observed on Qt 6.11.1 (qml, macOS 27, read through the Accessibility
# API): a Qt.labs.platform MenuBar in a plain Window is the system's menu
# bar; the items whose role is AboutRole, PreferencesRole or QuitRole go
# to the application menu whatever their text, and the first of each role
# takes the place; NoRole keeps an item called "About…" in its menu; an
# item or a menu that is not visible is not shown, and a menu left with
# nothing is not; Meta+W is ⌃W.
#
# @ljos-only: the oracle runs on Qt's offscreen and vnc platforms, which
# have no menu bar of the system's (see native-menu-bar.feature).

@ljos-only
Feature: Qt.labs.platform's MenuBar
  Qt.labs.platform has a MenuBar of the system's own, for any window,
  with Menu, MenuItem and MenuSeparator. Where the system has a menu bar,
  as macOS has, it is shown there; elsewhere it makes nothing.

  Background:
    Given the system shows menu bars of its own
    And the QML:
      """qml
      import QtQuick
      import Qt.labs.platform
      Window {
        id: window; objectName: "window"; width: 200; height: 100; visible: true
        property int made: 0
        property int quits: 0
        function change() { save.enabled = true; save.text = "Save"; hidden.visible = true }
        MenuBar {
          Menu {
            title: "&File"
            MenuItem { text: "&New"; shortcut: "Ctrl+N"; onTriggered: window.made++ }
            MenuItem { id: save; text: "Save &As…"; shortcut: "Ctrl+Shift+S"; enabled: false }
            MenuSeparator { }
            Menu { title: "Recent"; MenuItem { text: "one.txt" } }
            MenuItem { objectName: "wrap"; text: "Wrap"; checkable: true; checked: true; shortcut: "Meta+W" }
            MenuItem { id: hidden; text: "Hidden"; visible: false }
            MenuItem { text: "Einstellungen…"; role: MenuItem.PreferencesRole }
            MenuItem { text: "Beenden"; role: MenuItem.QuitRole; onTriggered: window.quits++ }
            MenuItem { text: "About the plan"; role: MenuItem.NoRole }
          }
          Menu { title: "Help"; MenuItem { text: "Exit" } }
          Menu { title: "Gone"; visible: false; MenuItem { text: "Away" } }
        }
      }
      """

  @api-Qt.labs.platform.MenuBar @api-Qt.labs.platform.MenuItem.role @api-Qt.labs.platform.MenuSeparator
  Scenario: The system's bar has the menus, items, separators and sub-menus, and the roles' items in the application menu
    Then the system's menu bar is:
      """
      Application
        Einstellungen…
        Beenden
      File
        New  ⌘N
        Save As…  ⇧⌘S  disabled
        ---
        Recent
          one.txt
        ✓ Wrap  ⌃W
        About the plan
      """

  @api-Qt.labs.platform.MenuItem.triggered
  Scenario: Choosing an item in the system's bar triggers it once
    When the user chooses "File > New" in the system's menu bar
    Then the property "made" of "window" is 1

  @api-Qt.labs.platform.MenuItem.checked
  Scenario: Choosing a checkable item turns it over
    When the user chooses "File > Wrap" in the system's menu bar
    Then the property "checked" of "wrap" is false
    And the system's menu bar shows "File > Wrap" unchecked

  @api-Qt.labs.platform.MenuItem.enabled
  Scenario: A disabled item cannot be chosen
    When the user chooses "File > Save As…" in the system's menu bar
    Then the system's menu bar shows "File > Save As…" disabled

  @api-Qt.labs.platform.MenuItem.visible @api-Qt.labs.platform.MenuItem.text
  Scenario: What changes in the QML changes in the system's bar
    When the method "change" of "window" is called
    Then the system's menu bar shows "File > Save" enabled
    And the system's menu bar shows "File > Hidden"

  @api-Qt.labs.platform.MenuItem.role
  Scenario: Quit in the application menu triggers the item whose role is QuitRole
    When the user chooses "Quit" in the system's application menu
    Then the property "quits" of "window" is 1
