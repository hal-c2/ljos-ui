# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-menubaritem.html
# Verified on Qt 6.11.2 (qml6, offscreen): an item that is given a menu
# takes its title unless the item's own text was given, and takes the
# title again each time it changes.

@qt-parity @api-QtQuick.Controls.MenuBarItem.menu @api-QtQuick.Controls.MenuBarItem.text
Feature: A menu bar item is named by its menu
  A MenuBarItem declared with a menu, in a bar or on its own, shows the
  menu's title.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        MenuBarItem { objectName: "lone"; menu: Menu { title: "Lone" } }
        MenuBar {
          MenuBarItem { objectName: "file"; menu: Menu { title: "File" } }
          MenuBarItem { objectName: "own"; text: "Own"; menu: Menu { title: "Edit" } }
        }
      }
      """

  Scenario Outline: The <item> item's text is <text>
    Then the property "text" of "<item>" is "<text>"

    Examples:
      | item | text |
      | lone | Lone |
      | file | File |
      | own  | Own  |

  Scenario: An item follows its menu's title
    When the property "menu.title" of "file" is set to "Changed"
    And the property "menu.title" of "lone" is set to "Moved"
    Then the property "text" of "file" is "Changed"
    And the property "text" of "lone" is "Moved"

  Scenario: An item whose text was written takes the next title
    When the property "text" of "file" is set to "Mine"
    Then the property "text" of "file" is "Mine"
    When the property "menu.title" of "file" is set to "Again"
    Then the property "text" of "file" is "Again"
