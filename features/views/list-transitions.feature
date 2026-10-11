# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-listview.html (add, remove, populate, displaced)
#   https://doc.qt.io/qt-6/qml-qtquick-viewtransition.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @api-QtQuick.ListView.add @api-QtQuick.ListView.populate
Feature: Transitions of a ListView
  A view runs a transition on the delegates of rows that are added, on the ones first shown
  and on the ones about to be removed.

  Background:
    Given the QML:
      """qml
      import QtQuick
      ListView {
        objectName: "list"
        width: 200; height: 300
        model: lm
        ListModel {
          id: lm
          objectName: "lm"
          ListElement { name: "a" }
          ListElement { name: "b" }
        }
        populate: Transition { NumberAnimation { property: "scale"; from: 0.5; to: 1; duration: 200 } }
        add: Transition { NumberAnimation { property: "opacity"; from: 0; to: 1; duration: 200 } }
        remove: Transition { NumberAnimation { property: "opacity"; to: 0; duration: 200 } }
        delegate: Item {
          required property string name
          objectName: name; width: 200; height: 20
        }
      }
      """

  Scenario: The rows shown first run the populate transition
    When 100 milliseconds pass
    Then the property "scale" of "a" is at least 0.6
    And the property "scale" of "a" is at most 0.9
    When 300 milliseconds pass
    Then the property "scale" of "a" is 1
    And the property "opacity" of "a" is 1

  Scenario: An added row runs the add transition
    When 400 milliseconds pass
    And the method "append" of "lm" is called with {"name": "c"}
    And 120 milliseconds pass
    Then the property "opacity" of "c" is at least 0.2
    And the property "opacity" of "c" is at most 0.9
    And the property "scale" of "c" is 1
    When 300 milliseconds pass
    Then the property "opacity" of "c" is 1

  Scenario: A removed row stays until its remove transition has ended
    When 400 milliseconds pass
    And the method "remove" of "lm" is called with 0
    And 120 milliseconds pass
    Then the object "a" exists
    And the property "opacity" of "a" is at most 0.8
    And the property "count" of "list" is 1
    When 300 milliseconds pass
    Then the object "a" does not exist
    And the property "y" of "b" is 0
