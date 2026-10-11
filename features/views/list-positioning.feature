# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-listview.html (positionViewAtIndex, positionViewAtBeginning,
#   positionViewAtEnd, indexAt, itemAt, itemAtIndex)
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @api-QtQuick.ListView.positionViewAtIndex @api-QtQuick.ListView.indexAt
Feature: Putting a row of a ListView where it can be seen
  `positionViewAtIndex` scrolls so that a row is at a place in the view; `indexAt` and `itemAt`
  tell what is at a place of the content.

  Background:
    Given the QML:
      """qml
      import QtQuick
      ListView {
        id: list
        objectName: "list"
        width: 200; height: 300
        model: 100
        function hasItem(i) { return list.itemAtIndex(i) !== null }
        function nameAt(i) { return list.itemAtIndex(i).objectName }
        function nameAtPoint(x, y) { var it = list.itemAt(x, y); return it ? it.objectName : "none" }
        property int found: -2
        property string name: ""
        function ask(x, y) { found = list.indexAt(x, y); name = nameAtPoint(x, y) }
        function has(i) { found = hasItem(i) ? 1 : 0 }
        delegate: Item { objectName: "r" + index; width: 200; height: 20 }
      }
      """

  Scenario Outline: A row is put at a place of the view
    When the property "contentY" of "list" is set to <from>
    And the method "positionViewAtIndex" of "list" is called with <index>, <mode>
    Then the property "contentY" of "list" is <contentY>

    Examples:
      | from | index | mode | contentY | what                                           |
      | 0    | 50    | 0    | 1000     | Beginning                                      |
      | 0    | 50    | 1    | 860      | Center                                         |
      | 0    | 50    | 2    | 720      | End                                            |
      | 0    | 50    | 3    | 720      | Visible: a row below comes to the end          |
      | 1500 | 10    | 3    | 200      | Visible: a row above comes to the beginning    |
      | 400  | 25    | 3    | 400      | Visible: a row in the view moves nothing       |
      | 0    | 30    | 4    | 320      | Contain                                        |
      | 0    | 0     | 1    | 0        | Center: no further than the beginning          |
      | 0    | 99    | 0    | 1700     | Beginning: no further than the end             |
      | 0    | 40    | 5    | 800      | SnapPosition                                   |
      | 600  | 500   | 0    | 600      | a row the model does not have                  |
      | 600  | -1    | 0    | 600      | a negative index                               |

  Scenario: The row put in place has an item at once
    When the method "positionViewAtIndex" of "list" is called with 70, 0
    Then the property "y" of "r70" is 1400
    And the object "r0" exists

  Scenario: To the end and back to the beginning
    When the method "positionViewAtEnd" of "list" is called
    Then the property "contentY" of "list" is 1700
    And the property "atYEnd" of "list" is true
    When the method "positionViewAtBeginning" of "list" is called
    Then the property "contentY" of "list" is 0
    And the property "atYBeginning" of "list" is true

  Scenario Outline: What is at a place of the content
    When the method "ask" of "list" is called with <x>, <y>
    Then the property "found" of "list" is <index>
    And the property "name" of "list" is "<name>"

    Examples:
      | x   | y    | index | name |
      | 10  | 0    | 0     | r0   |
      | 10  | 19   | 0     | r0   |
      | 10  | 20   | 1     | r1   |
      | 199 | 65   | 3     | r3   |
      | 10  | -1   | -1    | none |

  Scenario: A row far from the view has no item
    When the method "has" of "list" is called with 3
    Then the property "found" of "list" is 1
    When the method "has" of "list" is called with 80
    Then the property "found" of "list" is 0

  Scenario: Places follow the view when it has scrolled
    When the property "contentY" of "list" is set to 1000
    And the method "ask" of "list" is called with 10, 1005
    Then the property "found" of "list" is 50
    And the property "name" of "list" is "r50"
