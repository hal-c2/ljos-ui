# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-menu.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-menuitem.html
#
# What a resting or moving pointer does to a menu. The oracle cannot move the
# pointer over a popup, so these run on Ljós only; every value here was read
# off Qt 6.11.2 with Qt Test (mouseMove, mousePress, mouseRelease).

@ljos-only
Feature: Menu and the pointer
  The item under the pointer is the current one, and resting on the item of
  a sub-menu opens that menu.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        id: root
        objectName: "root"
        width: 400; height: 400
        property string log: ""
        Menu {
          objectName: "m"
          onClosed: root.log += "closed "
          MenuItem { objectName: "one"; text: "One"; onTriggered: root.log += "one " }
          MenuItem { objectName: "two"; text: "Two"; onTriggered: root.log += "two " }
          MenuItem { objectName: "off"; text: "Off"; enabled: false }
          MenuSeparator {}
          Menu { objectName: "sub"; title: "Sub"
                 MenuItem { objectName: "deep"; text: "Deep" } }
        }
      }
      """

  @api-QtQuick.Controls.MenuItem.highlighted
  Scenario: The item under the pointer is the current one
    When the method "open" of "m" is called
    And the user moves the pointer to 100, 60
    Then the property "currentIndex" of "m" is 1
    And the property "highlighted" of "two" is true
    And the property "activeFocus" of "two" is true
    And the property "highlighted" of "one" is false

  Scenario: The current item follows the pointer
    When the method "open" of "m" is called
    And the user moves the pointer to 100, 60
    And the user moves the pointer to 100, 20
    Then the property "currentIndex" of "m" is 0
    And the property "highlighted" of "two" is false

  Scenario: A disabled item under the pointer is not current
    When the method "open" of "m" is called
    And the user moves the pointer to 100, 60
    And the user moves the pointer to 100, 100
    Then the property "highlighted" of "off" is false
    And the property "currentIndex" of "m" is 1

  Scenario: A separator under the pointer changes nothing
    When the method "open" of "m" is called
    And the user moves the pointer to 100, 60
    And the user moves the pointer to 100, 126
    Then the property "currentIndex" of "m" is 1

  Scenario: The pointer leaving the menu leaves the current item as it is
    When the method "open" of "m" is called
    And the user moves the pointer to 100, 60
    And the user moves the pointer to 300, 300
    Then the property "currentIndex" of "m" is 1

  @api-QtQuick.Controls.Menu.addMenu
  Scenario: The pointer resting on the item of a sub-menu opens it after a moment
    When the method "open" of "m" is called
    And the user moves the pointer to 100, 150
    Then the property "currentIndex" of "m" is 4
    And the property "visible" of "sub" is false
    When 200 milliseconds pass
    Then the property "visible" of "sub" is false
    When 100 milliseconds pass
    Then the property "visible" of "sub" is true
    And the property "currentIndex" of "sub" is -1
    And the property "activeFocus" of "sub" is true

  Scenario: A short rest opens nothing
    When the method "open" of "m" is called
    And the user moves the pointer to 100, 150
    And 100 milliseconds pass
    And the user moves the pointer to 100, 20
    And 1 second passes
    Then the property "visible" of "sub" is false
    And the property "currentIndex" of "m" is 0

  Scenario: The pointer makes an item of the sub-menu current
    When the method "open" of "m" is called
    And the user moves the pointer to 100, 150
    And 300 milliseconds pass
    And the user moves the pointer to 300, 150
    Then the property "currentIndex" of "sub" is 0
    And the property "highlighted" of "deep" is true
    And the property "currentIndex" of "m" is 4

  Scenario: Moving to another item of the menu closes the sub-menu at once
    When the method "open" of "m" is called
    And the user moves the pointer to 100, 150
    And 300 milliseconds pass
    And the user moves the pointer to 100, 20
    Then the property "visible" of "sub" is false
    And the property "visible" of "m" is true
    And the property "currentIndex" of "m" is 0

  Scenario: A press makes the item current before it is let go
    When the method "open" of "m" is called
    And the user presses the pointer at 100, 60
    Then the property "currentIndex" of "m" is 1
    And the property "down" of "two" is true
    And the property "visible" of "m" is true

  Scenario: A press dragged onto another item triggers nothing
    When the method "open" of "m" is called
    And the user presses the pointer at 100, 20
    And the user moves the pointer to 100, 60
    And the user releases the pointer at 100, 60
    Then the property "log" of "root" is ""
    And the property "visible" of "m" is true
