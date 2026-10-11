# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-listview.html (attached properties ListView.view, isCurrentItem,
#   delayRemove; attached signals add and remove)
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @api-QtQuick.ListView.view @api-QtQuick.ListView.add @api-QtQuick.ListView.remove
Feature: What a ListView attaches to its delegates
  The root item of a delegate knows its view, and is told when its row was added or is about
  to be removed.

  Background:
    Given the QML:
      """qml
      import QtQuick
      ListView {
        id: list
        objectName: "list"
        width: 200; height: 300
        property string log: ""
        model: lm
        ListModel {
          id: lm
          objectName: "lm"
          ListElement { name: "a" }
          ListElement { name: "b" }
          ListElement { name: "c" }
        }
        delegate: Item {
          required property string name
          required property int index
          objectName: name; width: 200; height: 20
          property string view: ListView.view.objectName
          property bool hold: false
          ListView.delayRemove: hold
          ListView.onAdd: list.log += "+" + name + index
          ListView.onRemove: list.log += "-" + name + index
        }
      }
      """
    When 100 milliseconds pass

  Scenario: A delegate knows its view
    Then the property "view" of "b" is "list"

  Scenario: Rows the model had at the start are not announced as added
    Then the property "log" of "list" is ""

  Scenario: A row added later is announced to its delegate
    When the method "insert" of "lm" is called with 1, {"name": "x"}
    And 100 milliseconds pass
    Then the property "log" of "list" is "+x1"

  Scenario: A row about to be removed is announced to its delegate
    When the method "remove" of "lm" is called with 1
    And 100 milliseconds pass
    Then the property "log" of "list" is "-b-1"
    And the object "b" does not exist

  Scenario: Rows outside the view are not announced
    When the property "height" of "list" is set to 40
    And the property "cacheBuffer" of "list" is set to 0
    And 100 milliseconds pass
    And the method "append" of "lm" is called with {"name": "far"}
    And 100 milliseconds pass
    Then the property "log" of "list" is ""

  Scenario: A delegate that delays its removal stays until it lets go
    When the property "hold" of "b" is set to true
    And the method "remove" of "lm" is called with 1
    And 100 milliseconds pass
    Then the object "b" exists
    And the property "count" of "list" is 2
    When the property "hold" of "b" is set to false
    And 100 milliseconds pass
    Then the object "b" does not exist
    And the property "y" of "c" is 20
