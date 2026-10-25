# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-contextmenu.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-menu.html#popup-method
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A context menu attached to an item
  `ContextMenu.menu` is the menu an item opens when the user asks for
  one with a right-click: on the item, where the click was, kept inside
  the window. `ContextMenu.requested(position)` tells where the click
  was, in the item's coordinates, before the menu opens. An item under
  the pointer with neither passes the request on to the one beneath; a
  MouseArea or a TapHandler that takes the right button keeps it for
  itself.

  # The state of the menus is read when a step asks for it: qml6 breaks a
  # context menu that a binding reads before it is first opened.
  Background:
    Given the file "MenuState.qml":
      """qml
      import QtQuick
      import QtQuick.Controls
      Item {
        property Item eats
        property Item own
        property bool shown
        property bool onTheItem
        property real menuX
        property real menuY
        property bool ownShown
        function look() {
          const m = eats.ContextMenu.menu
          shown = m.opened; onTheItem = m.parent === eats; menuX = m.x; menuY = m.y
          ownShown = own.ContextMenu.menu.opened
        }
      }
      """
    And the file "Keepers.qml":
      """qml
      import QtQuick
      import QtQuick.Controls
      Item {
        id: keepers
        property string log: ""
        Rectangle {
          x: 20; y: 150; width: 60; height: 60
          ContextMenu.onRequested: keepers.log += "under;"
          MouseArea { anchors.fill: parent; acceptedButtons: Qt.RightButton; onClicked: keepers.log += "area;" }
        }
        Rectangle {
          x: 100; y: 150; width: 60; height: 60
          ContextMenu.onRequested: keepers.log += "under;"
          TapHandler { acceptedButtons: Qt.RightButton; onTapped: keepers.log += "tap;" }
        }
      }
      """
    # qml6 stops answering when a menu is pressed in the test harness, so
    # `choose` clicks the item from QML.
    And the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        id: root; objectName: "root"
        width: 400; height: 400
        property string log: ""
        function choose() { eat.click() }
        Rectangle {
          x: 20; y: 20; width: 100; height: 100
          ContextMenu.onRequested: (p) => root.log += "told " + p.x + "," + p.y + ";"
        }
        Rectangle {
          id: eats; x: 220; y: 150; width: 100; height: 100
          ContextMenu.menu: Menu { MenuItem { id: eat; text: "Eat"; onTriggered: root.log += "eat;" } }
          Rectangle { x: 50; y: 50; width: 50; height: 50 }
        }
        Rectangle {
          id: own; x: 220; y: 20; width: 100; height: 100
          ContextMenu.menu: Menu { MenuItem { text: "Own" } }
          ContextMenu.onRequested: (p) => root.log += "own " + p.x + "," + p.y + ";"
        }
        Keepers { objectName: "keepers"; width: 400; height: 400 }
        MenuState { objectName: "state"; eats: eats; own: own }
      }
      """

  @api-QtQuick.Controls.ContextMenu.requested
  Scenario: A right-click asks for the item's context menu where it was
    When the user clicks the right button at 50, 60
    Then the property "log" of "root" is "told 30,40;"

  Scenario: A left click asks for no context menu
    When the user presses the pointer at 50, 60
    And the user releases the pointer at 50, 60
    Then the property "log" of "root" is ""

  @api-QtQuick.Controls.ContextMenu.menu
  Scenario: The menu opens on the item, at the click, inside the window
    When the user clicks the right button at 250, 180
    And the method "look" of "state" is called
    Then the property "shown" of "state" is true
    And the property "onTheItem" of "state" is true
    And the property "menuY" of "state" is 30
    And the property "menuX" of "state" is -20

  Scenario: An item with no menu of its own passes the request to the one beneath
    When the user clicks the right button at 290, 220
    And the method "look" of "state" is called
    Then the property "shown" of "state" is true
    And the property "menuY" of "state" is 70

  Scenario: An item that is told of the request opens its menu as well
    When the user clicks the right button at 250, 50
    And the method "look" of "state" is called
    Then the property "log" of "root" is "own 30,30;"
    And the property "ownShown" of "state" is true

  Scenario: A MouseArea that takes the right button keeps the click
    When the user clicks the right button at 50, 180
    Then the property "log" of "keepers" is "area;"

  Scenario: A TapHandler that takes the right button keeps the click
    When the user clicks the right button at 130, 180
    Then the property "log" of "keepers" is "tap;"

  Scenario: Choosing an item closes the menu
    When the user clicks the right button at 250, 180
    And the method "choose" of "root" is called
    And the method "look" of "state" is called
    Then the property "log" of "root" is "eat;"
    And the property "shown" of "state" is false
