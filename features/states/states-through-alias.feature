# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#states-prop
#   https://doc.qt.io/qt-6/qtqml-syntax-objectattributes.html#property-aliases
# Verified on Qt 6.11.2 (qml6, offscreen).
# The coffee demo gives its cup its states this way, from the page that
# holds the cup.

@qt-parity @api-QtQuick.Item.states
Feature: States given through an alias
  A component that shows one of its items through an alias lets whoever
  uses it give that item states: they are the item's, not the component's.

  Background:
    Given the file "Box.qml":
      """qml
      import QtQuick
      Item {
        property alias cup: cup
        Item { id: cup; property int amount: 0 }
      }
      """
    And the file "Main.qml":
      """qml
      import QtQuick
      Item {
        objectName: "root"
        width: 100; height: 100
        property string want: ""
        Box {
          id: b
          objectName: "b"
          cup.objectName: "cup"
          cup.state: want
          cup.states: State { name: "full"; PropertyChanges { target: b.cup; amount: 80 } }
          cup.transitions: Transition {
            NumberAnimation { target: b.cup; property: "amount"; duration: 200 }
          }
        }
      }
      """

  Scenario: The item enters a state given through the alias
    When "Main.qml" is loaded
    And the property "want" of "root" is set to "full"
    And 300 milliseconds pass
    Then the property "amount" of "cup" is 80
    And the property "state" of "b" is ""

  Scenario: The transition given through the alias animates it
    When "Main.qml" is loaded
    And the property "want" of "root" is set to "full"
    And 100 milliseconds pass
    Then the property "amount" of "cup" is at least 20
    And the property "amount" of "cup" is at most 60
