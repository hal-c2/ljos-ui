# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-menubar.html ("Native menu bars")
#   https://doc.qt.io/qt-6/qml-qt-labs-platform-menubar.html
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

  @api-Ljos.SystemMenu.role
  Scenario: An empty menu titled "Window" beside a menu with the role is not shown
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      import Ljos
      ApplicationWindow {
        width: 200; height: 100; visible: true
        menuBar: MenuBar {
          Menu { title: "Window" }
          Menu { title: "Fenster"; SystemMenu.role: SystemMenu.WindowRole }
        }
      }
      """
    Then the system lists the windows in the menu "Fenster"
    And the system's menu bar does not show "Window"

  @api-Qt.labs.platform.MenuBar @api-Ljos.SystemMenu.role
  Scenario: A Qt.labs.platform bar's menu with the window role is the one, and an empty "Window" beside it is not shown
    Given the QML:
      """qml
      import QtQuick
      import Qt.labs.platform
      import Ljos
      Window {
        objectName: "window"; width: 200; height: 100; visible: true
        MenuBar {
          Menu { title: "Window" }
          Menu { title: "Fenster"; SystemMenu.role: SystemMenu.WindowRole }
        }
      }
      """
    When the method "requestActivate" of "window" is called
    And 50 milliseconds pass
    Then the system lists the windows in the menu "Fenster"
    And the system's menu bar shows "Fenster"
    And the system's menu bar does not show "Window"
