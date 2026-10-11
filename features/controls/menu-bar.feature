# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-menubar.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-menubaritem.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-menu.html
# Verified on Qt 6.11.2 (qml6, offscreen).

# TODO(menubar): MenuBar and MenuBarItem are not built; a Menu is opened by
# the application until they are.
@qt-parity @backlog
Feature: MenuBar
  A menu bar is a row of the titles of its menus; choosing a title opens
  its menu under it.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        id: root
        objectName: "root"
        width: 400; height: 400
        property string seen: ""
        function look() {
          seen = bar.itemAt(0).text + "," + bar.menuAt(1).title + ","
               + bar.menuAt(0).visible + "," + bar.menuAt(1).visible
        }
        MenuBar {
          id: bar; objectName: "bar"
          width: 400
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
