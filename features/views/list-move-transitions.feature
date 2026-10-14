# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-listview.html (move, displaced, addDisplaced, removeDisplaced, moveDisplaced)
#   https://doc.qt.io/qt-6/qml-qtquick-gridview.html
#   https://doc.qt.io/qt-6/qml-qtquick-viewtransition.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @api-QtQuick.ListView.move @api-QtQuick.ListView.displaced @api-QtQuick.ListView.addDisplaced @api-QtQuick.ListView.removeDisplaced @api-QtQuick.ListView.moveDisplaced @api-QtQuick.GridView.move @api-QtQuick.ViewTransition.destination
Feature: Move and displaced transitions of a view
  When rows of the model move, the items that moved run the move transition and the items
  pushed aside by the change run a displaced transition: the one named for the change that
  displaced them, or the plain displaced transition when there is none.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 200; height: 460
        property real moveDestY: -1; property int moveIdx: -1
        property int moveTarget: -1; property string moveItem: ""
        function useRemoveSpecific() { list.removeDisplaced = rd } function useAddSpecific() { list.addDisplaced = rd } function useMoveDispSpecific() { list.moveDisplaced = rd }
        ListModel { id: lm; objectName: "lm"; ListElement { name: "a" } ListElement { name: "b" } ListElement { name: "c" } ListElement { name: "d" } }
        ListView {
          id: list; objectName: "list"; width: 200; height: 300; model: lm
          move: Transition { id: mv
            ScriptAction { script: { root.moveDestY = mv.ViewTransition.destination.y; root.moveIdx = mv.ViewTransition.index; root.moveTarget = mv.ViewTransition.targetIndexes[0]; root.moveItem = mv.ViewTransition.item.objectName } }
            NumberAnimation { properties: "x,y"; duration: 400 } }
          displaced: Transition { NumberAnimation { properties: "x,y"; duration: 400 } NumberAnimation { property: "opacity"; from: 0.2; to: 1; duration: 400 } }
          Transition { id: rd; NumberAnimation { properties: "x,y"; duration: 400 } }
          delegate: Item { required property string name; objectName: name; width: 200; height: 20 }
        }
        ListModel { id: gm; objectName: "gm"; ListElement { name: "a" } ListElement { name: "b" } ListElement { name: "c" } ListElement { name: "d" } }
        GridView {
          objectName: "grid"; y: 320; width: 200; height: 120; cellWidth: 100; cellHeight: 20; model: gm
          move: Transition { NumberAnimation { properties: "x,y"; duration: 400 } }
          displaced: Transition { NumberAnimation { properties: "x,y"; duration: 400 } }
          delegate: Item { required property string name; objectName: "g" + name; width: 100; height: 20 }
        }
      }
      """
    When 100 milliseconds pass

  Scenario: A moved row runs the move transition and the rows it passes are displaced
    When the method "move" of "lm" is called with 0, 2, 1
    And 150 milliseconds pass
    Then the property "y" of "a" is at least 4
    And the property "y" of "a" is at most 36
    And the property "y" of "b" is at least 4
    And the property "y" of "b" is at most 16
    And the property "opacity" of "a" is 1
    And the property "opacity" of "b" is at most 0.9
    When 500 milliseconds pass
    Then the property "y" of "a" is 40
    And the property "y" of "b" is 0
    And the property "y" of "c" is 20
    And the property "opacity" of "b" is 1

  Scenario: The move transition is told the item, its index and where it goes
    When the method "move" of "lm" is called with 0, 2, 1
    And 150 milliseconds pass
    Then the property "moveItem" of "root" is "a"
    And the property "moveIdx" of "root" is 2
    And the property "moveTarget" of "root" is 2
    And the property "moveDestY" of "root" is 40

  Scenario: Rows displaced by a removal run the displaced transition
    When the method "remove" of "lm" is called with 0
    And 150 milliseconds pass
    Then the property "y" of "b" is at least 4
    And the property "y" of "b" is at most 16
    And the property "opacity" of "b" is at most 0.9
    And the property "count" of "list" is 3
    When 500 milliseconds pass
    Then the property "y" of "b" is 0
    And the property "opacity" of "b" is 1

  Scenario: A removeDisplaced transition is run instead of the displaced one
    When the method "useRemoveSpecific" of "root" is called
    And the method "remove" of "lm" is called with 0
    And 150 milliseconds pass
    Then the property "y" of "b" is at least 4
    And the property "y" of "b" is at most 16
    And the property "opacity" of "b" is 1
    When 500 milliseconds pass
    Then the property "y" of "b" is 0

  Scenario: Rows displaced by an insertion run the displaced transition
    When the method "insert" of "lm" is called with 0, {"name": "e"}
    And 150 milliseconds pass
    Then the property "y" of "a" is at least 4
    And the property "y" of "a" is at most 16
    And the property "opacity" of "a" is at most 0.9
    And the property "y" of "e" is 0
    When 500 milliseconds pass
    Then the property "y" of "a" is 20
    And the property "opacity" of "a" is 1

  Scenario: An addDisplaced transition is run instead of the displaced one
    When the method "useAddSpecific" of "root" is called
    And the method "insert" of "lm" is called with 0, {"name": "e"}
    And 150 milliseconds pass
    Then the property "y" of "a" is at least 4
    And the property "y" of "a" is at most 16
    And the property "opacity" of "a" is 1
    When 500 milliseconds pass
    Then the property "y" of "a" is 20

  Scenario: A moveDisplaced transition is run instead of the displaced one
    When the method "useMoveDispSpecific" of "root" is called
    And the method "move" of "lm" is called with 0, 2, 1
    And 150 milliseconds pass
    Then the property "y" of "b" is at least 4
    And the property "y" of "b" is at most 16
    And the property "opacity" of "b" is 1
    When 500 milliseconds pass
    Then the property "y" of "b" is 0

  Scenario: A grid moves its cells in both directions
    When the method "move" of "gm" is called with 0, 2, 1
    And 150 milliseconds pass
    Then the property "y" of "ga" is at least 2
    And the property "y" of "ga" is at most 18
    And the property "x" of "gb" is at least 10
    And the property "x" of "gb" is at most 90
    When 500 milliseconds pass
    Then the property "x" of "ga" is 0
    And the property "y" of "ga" is 20
    And the property "x" of "gb" is 0
    And the property "y" of "gb" is 0
    And the property "x" of "gc" is 100
    And the property "y" of "gc" is 0
