# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-menu.html
#   https://doc.qt.io/qt-6/qml-qtquick-flickable.html
#
# The oracle cannot hold the pointer down over a popup: what tells it that a
# step has arrived is under the popup. These run on Ljós only; every value
# here was read off Qt 6.11.2 with Qt Test (mousePress, mouseMove,
# mouseRelease). `menu.feature` has what the oracle can see of a drag.

@ljos-only
Feature: Dragging the items of a menu that scrolls
  A menu with more items than fit shows them in a view. A pointer that is
  pressed on an item and moved takes the items along instead of choosing.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        id: root; objectName: "root"
        width: 400; height: 400
        property string chosen: ""
        Menu {
          objectName: "m"
          Repeater {
            model: 20
            MenuItem { text: "Item " + index; onTriggered: root.chosen = text }
          }
        }
      }
      """

  Scenario: The first move past the threshold moves nothing yet
    When the method "open" of "m" is called
    And the user presses the pointer at 100, 300
    And the user moves the pointer to 100, 280
    Then the property "contentItem.contentY" of "m" is 0

  Scenario: The items follow the pointer from the second move on
    When the method "open" of "m" is called
    And the user presses the pointer at 100, 300
    And the user moves the pointer to 100, 280
    And the user moves the pointer to 100, 270
    And the user moves the pointer to 100, 170
    Then the property "contentItem.contentY" of "m" is 100

  Scenario: Letting go after a drag chooses nothing and leaves the menu open
    When the method "open" of "m" is called
    And the user presses the pointer at 100, 300
    And the user moves the pointer to 100, 280
    And the user moves the pointer to 100, 270
    And the user moves the pointer to 100, 170
    And 500 milliseconds pass
    And the user releases the pointer at 100, 170
    Then the property "contentItem.contentY" of "m" is 100
    And the property "visible" of "m" is true
    And the property "chosen" of "root" is ""
