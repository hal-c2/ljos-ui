# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-menubar.html ("Native menu bars")
#   https://doc.qt.io/qt-6/qaction.html#MenuRole-enum (TextHeuristicRole, NoRole)
#   https://developer.apple.com/documentation/appkit/nsapplication/windowsmenu
#
# @ljos-only: Qt has no role that makes a menu the one the system lists
# the windows in, and the oracle has no menu bar of the system's (see
# native-menu-bar.feature).

@ljos-only
Feature: The menu the system lists the windows in
  Where the system has a menu bar of its own, as macOS has, it lists the
  application's windows in one of the bar's menus. That is the menu whose
  SystemMenu.role is SystemMenu.WindowRole, or else the one titled
  "Window", so a menu whose title is translated says it. Such a menu is
  shown even with nothing of its own, for the system to fill.

  Background:
    Given the system shows menu bars of its own

  @api-QtQuick.Controls.MenuBar
  Scenario: The menu titled "Window" is the one
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      ApplicationWindow {
        width: 200; height: 100; visible: true
        menuBar: MenuBar {
          Menu { title: "&File"; Action { text: "Open" } }
          Menu { title: "&Window"; Action { text: "Minimize" } }
        }
      }
      """
    Then the system lists the windows in the menu "Window"

  @api-Ljos.SystemMenu.role
  Scenario: A menu with the window role is the one whatever its title, and is shown with nothing in it
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      import Ljos
      ApplicationWindow {
        width: 200; height: 100; visible: true
        menuBar: MenuBar {
          Menu { title: "Datei"; Action { text: "Öffnen" } }
          Menu { title: "Fenster"; SystemMenu.role: SystemMenu.WindowRole }
        }
      }
      """
    Then the system lists the windows in the menu "Fenster"
    And the system's menu bar shows "Fenster"

  @api-Ljos.SystemMenu.role
  Scenario: The role is taken over a menu's title
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      import Ljos
      ApplicationWindow {
        width: 200; height: 100; visible: true
        menuBar: MenuBar {
          Menu { title: "Window"; Action { text: "Tile" } }
          Menu { title: "Windows"; SystemMenu.role: SystemMenu.WindowRole }
        }
      }
      """
    Then the system lists the windows in the menu "Windows"

  @api-Ljos.SystemMenu.role
  Scenario: A menu titled "Window" with no role is not the one, and is not shown empty
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      import Ljos
      ApplicationWindow {
        width: 200; height: 100; visible: true
        menuBar: MenuBar {
          Menu { title: "File"; Action { text: "Open" } }
          Menu { title: "Window"; SystemMenu.role: SystemMenu.NoRole }
        }
      }
      """
    Then the system lists the windows in no menu
    And the system's menu bar does not show "Window"

  @api-QtQuick.Controls.MenuBar
  Scenario: A bar with neither lists the windows in no menu
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      ApplicationWindow {
        width: 200; height: 100; visible: true
        menuBar: MenuBar {
          Menu { title: "File"; Action { text: "Open" } }
          Menu { title: "Fenster"; Action { text: "Minimieren" } }
        }
      }
      """
    Then the system lists the windows in no menu
