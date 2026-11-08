# Sources:
#   https://doc.qt.io/qt-6/qml-qt-labs-platform-menubar.html (window: the
#   window found through the QML parents unless it is set)
#   https://doc.qt.io/qt-6/qml-qtquick-window.html (active)
#
# @ljos-only: the oracle has no menu bar of the system's (see
# native-menu-bar.feature).

@ljos-only
Feature: Which window's Qt.labs.platform MenuBar the system shows
  A Qt.labs.platform MenuBar belongs to the window it is declared in, or
  to the one its window property names, and the system shows the bar of
  the active window. Where the system has no menu bar of its own, the bar
  makes nothing and says nothing.

  @api-Qt.labs.platform.MenuBar.window @api-QtQuick.Window.active
  Scenario: The active window's bar is the one shown
    Given the system shows menu bars of its own
    And the QML:
      """qml
      import QtQuick
      import Qt.labs.platform
      Item {
        Window {
          objectName: "a"; width: 200; height: 100; visible: true
          Item { MenuBar { Menu { title: "Draw"; MenuItem { text: "Line" } } } }
        }
        Window {
          id: b; objectName: "b"; width: 200; height: 100; visible: true
        }
        MenuBar { window: b; Menu { title: "Write"; MenuItem { text: "Word" } } }
      }
      """
    When the method "requestActivate" of "a" is called
    And 50 milliseconds pass
    Then the system's menu bar shows "Draw > Line"
    When the method "requestActivate" of "b" is called
    And 50 milliseconds pass
    Then the system's menu bar shows "Write > Word"
    And the system's menu bar does not show "Draw"

  @api-Qt.labs.platform.MenuBar
  Scenario: Where the system has no menu bar of its own the bar makes nothing
    Given the QML:
      """qml
      import QtQuick
      import Qt.labs.platform
      Window {
        objectName: "window"; width: 200; height: 100; visible: true
        MenuBar { Menu { title: "File"; MenuItem { text: "Open" } } }
      }
      """
    Then the system shows no menu bar
    And the log has no warnings
