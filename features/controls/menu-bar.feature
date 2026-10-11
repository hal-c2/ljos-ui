# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-menubar.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-menubaritem.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-menu.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: MenuBar
  A menu bar is a row of the titles of its menus; choosing a title opens
  its menu under it. What an item of the bar does is in
  menu-bar-item.feature.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        id: root; objectName: "root"; width: 400; height: 400
        property string seen: ""; property string titles: ""
        property bool fits: bar.contentWidth > 0 && bar.implicitWidth === bar.contentWidth
        function look() {
          seen = bar.itemAt(0).text + "," + bar.menuAt(1).title + ","
               + bar.menuAt(0).visible + "," + bar.menuAt(1).visible
        }
        function list() {
          titles = ""
          for (var i = 0; i < bar.count; ++i) titles += bar.itemAt(i).text + " "
        }
        function swap() { bar.addMenu(bar.takeMenu(0)) }
        function front() { bar.insertMenu(0, bar.takeMenu(1)) }
        function drop() { bar.removeMenu(bar.menuAt(1)) }
        function beyond() { titles = bar.menuAt(7) + " " + bar.itemAt(5) }
        function rename() { bar.menuAt(0).title = "Record" }
        MenuBar {
          id: bar; objectName: "bar"; width: 400
          Menu { objectName: "file"; title: "File"; MenuItem { text: "New" } }
          Menu { objectName: "edit"; title: "Edit"; MenuItem { text: "Cut" } }
        }
      }
      """

  @api-QtQuick.Controls.MenuBar
  Scenario: A menu bar has an item for every menu
    Then the property "count" of "bar" is 2
    And the property "menus.length" of "bar" is 2

  @api-QtQuick.Controls.MenuBar.menuAt
  Scenario: An item of the bar shows the title of its menu
    When the method "look" of "root" is called
    Then the property "seen" of "root" is "File,Edit,false,false"

  @api-QtQuick.Controls.MenuBar.height
  Scenario: A menu bar is as high as its items
    Then the property "implicitHeight" of "bar" is 40
    And the property "spacing" of "bar" is 0

  @api-QtQuick.Controls.MenuBarItem.menu
  Scenario: A click on a title opens its menu
    When the user touches 10, 20
    And the method "look" of "root" is called
    Then the property "seen" of "root" is "File,Edit,true,false"

  @api-QtQuick.Controls.MenuBar.addMenu
  Scenario: A menu taken out of the bar leaves one item
    When the method "takeMenu" of "bar" is called with 0
    Then the property "count" of "bar" is 1

  @api-QtQuick.Controls.MenuBar.contentItem @api-QtQuick.Controls.MenuBar.background
  Scenario Outline: The <property> of a menu bar is <value>
    Then the property "<property>" of "bar" is <value>

    Examples:
      | property                  | value     |
      | height                    | 40        |
      | contentHeight             | 40        |
      | padding                   | 0         |
      | focusPolicy               | 2         |
      | hoverEnabled              | true      |
      | activeFocusOnTab          | false     |
      | contentItem.x             | 0         |
      | contentItem.width         | 400       |
      | contentItem.height        | 40        |
      | contentItem.spacing       | 0         |
      | background.color          | "#e0e0e0" |
      | background.width          | 400       |
      | background.implicitHeight | 40        |

  @api-QtQuick.Controls.MenuBar.contentWidth
  Scenario: A menu bar asks for the width of its items
    Then the property "fits" of "root" is true

  @api-QtQuick.Controls.MenuBar.takeMenu
  Scenario: The item of a menu that was taken out is gone
    When the method "takeMenu" of "bar" is called with 0
    And the method "list" of "root" is called
    Then the property "titles" of "root" is "Edit "
    And the property "menus.length" of "bar" is 1

  @api-QtQuick.Controls.MenuBar.addMenu
  Scenario: A menu that is added goes to the end
    When the method "swap" of "root" is called
    And the method "list" of "root" is called
    Then the property "titles" of "root" is "Edit File "
    And the property "count" of "bar" is 2

  @api-QtQuick.Controls.MenuBar.insertMenu
  Scenario: A menu that is inserted goes where it is told
    When the method "front" of "root" is called
    And the method "list" of "root" is called
    Then the property "titles" of "root" is "Edit File "

  @api-QtQuick.Controls.MenuBar.removeMenu
  Scenario: A menu that is removed leaves the bar
    When the method "drop" of "root" is called
    And the method "list" of "root" is called
    Then the property "titles" of "root" is "File "
    And the property "count" of "bar" is 1

  @api-QtQuick.Controls.MenuBar.menuAt @api-QtQuick.Controls.MenuBar.itemAt
  Scenario: There is no menu and no item beyond the last
    When the method "beyond" of "root" is called
    Then the property "titles" of "root" is "null null"

  Scenario: An item follows the title of its menu
    When the method "rename" of "root" is called
    And the method "list" of "root" is called
    Then the property "titles" of "root" is "Record Edit "
