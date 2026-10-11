# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-parentchange.html
#   https://doc.qt.io/qt-6/qtquick-statesanimations-states.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: ParentChange
  A ParentChange gives an item another parent while a state lasts, and
  keeps the item where it is on the screen unless it says where to put it.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 200; height: 200
        property string under: inner.parent === away ? "away" : "holder"
        Item { id: holder; x: 50; y: 60; width: 100; height: 100
          Rectangle { id: inner; objectName: "inner"; x: 10; y: 10; width: 20; height: 20 } }
        Item { id: away; x: 5; y: 5; width: 50; height: 50 }
        states: [
          State { name: "moved"
            ParentChange { target: inner; parent: away } },
          State { name: "placed"
            ParentChange { target: inner; parent: away; x: 1; y: 2; width: 30; rotation: 45 } }
        ]
      }
      """

  @api-QtQuick.ParentChange.parent
  Scenario: The item has the parent the state gives it
    When the property "state" of "root" is set to "moved"
    Then the property "under" of "root" is "away"

  Scenario Outline: The item stays where it was on the screen: <property>
    When the property "state" of "root" is set to "moved"
    Then the property "<property>" of "inner" is <value>

    Examples:
      | property | value |
      | x        | 55    |
      | y        | 65    |

  Scenario Outline: Leaving the state puts the item back: <property>
    When the property "state" of "root" is set to "moved"
    And the property "state" of "root" is set to ""
    Then the property "<property>" of "<object>" is <value>

    Examples:
      | object | property | value    |
      | root   | under    | "holder" |
      | inner  | x        | 10       |
      | inner  | y        | 10       |

  Scenario Outline: A place the change names is the item's place in its new parent: <property>
    When the property "state" of "root" is set to "placed"
    Then the property "<property>" of "inner" is <value>

    Examples:
      | property | value |
      | x        | 1     |
      | y        | 2     |
      | width    | 30    |
      | height   | 20    |
      | rotation | 45    |

  Scenario Outline: What the change named is put back with the parent: <property>
    When the property "state" of "root" is set to "placed"
    And the property "state" of "root" is set to ""
    Then the property "<property>" of "inner" is <value>

    Examples:
      | property | value |
      | x        | 10    |
      | width    | 20    |
      | rotation | 0     |
