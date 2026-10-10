# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-keys.html#forwardTo-prop
#   https://doc.qt.io/qt-6/qml-qtquick-keys.html#priority-prop
# Verified on Qt 6.11.2 (qml6, offscreen; key input over its vnc platform).

@qt-parity
Feature: Keys.forwardTo
  An item can hand its key events to other items first, such as a text field
  that lets a list beside it see the arrow keys.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property string log: ""
        Item {
          id: list
          objectName: "list"
          property bool take: false
          Keys.onPressed: (event) => { root.log += "list "; event.accepted = take }
          Keys.onReleased: (event) => root.log += "list-up "
        }
        Item { id: bar; Keys.onPressed: (event) => root.log += "bar " }
        Item {
          objectName: "field"; focus: true
          Keys.forwardTo: [list, bar]
          Keys.onPressed: (event) => root.log += "field "
          Keys.onReleased: (event) => root.log += "field-up "
        }
        property int order: list.Keys.priority
      }
      """

  Scenario: The items named get the event first, in order
    When the user presses "A"
    Then the property "log" of "root" is "list bar field list-up field-up "

  Scenario: An item that accepts the forwarded press ends it
    When the property "take" of "list" is set to true
    And the user presses "A"
    Then the property "log" of "root" is "list list-up field-up "

  Scenario: Keys handlers run before the item's own key handling by default
    Then the property "order" of "root" is 0
