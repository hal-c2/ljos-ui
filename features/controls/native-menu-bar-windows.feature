# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-menubar.html ("Native menu bars")
#   https://doc.qt.io/qt-6/qml-qtquick-window.html (active, closing)
# Observed on Qt 6.11.1 (qml, macOS 27): with no "About" or "Quit" item
# the application menu still has Quit (⌘Q).
#
# @ljos-only: the oracle has no menu bar of the system's (see
# native-menu-bar.feature).

@ljos-only
Feature: Which window's menu bar the system shows
  The system has one menu bar, and it shows the bar of the active window.
  Where the system has no menu bar of its own, each bar is in its window.

  @api-QtQuick.Controls.MenuBar @api-QtQuick.Window.active
  Scenario: The active window's bar is the one shown
    Given the system shows menu bars of its own
    And the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        ApplicationWindow {
          objectName: "a"; width: 200; height: 100; visible: true; transientParent: null
          menuBar: MenuBar { Menu { title: "Draw"; Action { text: "Line" } } }
        }
        ApplicationWindow {
          objectName: "b"; width: 200; height: 100; visible: true; transientParent: null
          menuBar: MenuBar { Menu { title: "Write"; Action { text: "Word" } } }
        }
      }
      """
    When the method "requestActivate" of "a" is called
    And 50 milliseconds pass
    Then the system's menu bar shows "Draw > Line"
    When the method "requestActivate" of "b" is called
    And 50 milliseconds pass
    Then the system's menu bar shows "Write > Word"
    And the system's menu bar does not show "Draw"

  @api-QtQuick.Window.closing
  Scenario: Quit with no item that quits closes the windows as their user would
    Given the system shows menu bars of its own
    And the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      ApplicationWindow {
        id: window; objectName: "window"; width: 200; height: 100; visible: true
        property int asked: 0
        onClosing: (close) => asked++
        menuBar: MenuBar { Menu { title: "File"; Action { text: "Open" } } }
      }
      """
    When the user chooses "Quit" in the system's application menu
    Then the property "asked" of "window" is 1
    And the property "visible" of "window" is false

  @api-QtQuick.Controls.MenuBar
  Scenario: Where the system has no menu bar of its own the bar is in the window
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      ApplicationWindow {
        id: window; objectName: "window"; width: 200; height: 100; visible: true
        property bool inWindow: bar.height > 0 && contentItem.y === bar.height
        menuBar: MenuBar { id: bar; Menu { title: "File"; Action { text: "Open" } } }
      }
      """
    Then the property "inWindow" of "window" is true
    And the system shows no menu bar
