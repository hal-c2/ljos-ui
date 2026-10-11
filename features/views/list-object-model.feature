# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-models-objectmodel.html
#   https://doc.qt.io/qt-6/qml-qtquick-listview.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @api-QtQml.Models.ObjectModel.count @api-QtQml.Models.ObjectModel.children
Feature: A view shows the items of an ObjectModel
  The rows of an ObjectModel are items that exist already. A view shows each of them where its
  row is and makes no delegate of its own.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQml.Models
      Item {
        id: root; objectName: "root"
        width: 200; height: 300
        property string seen: ""
        function add() { rows.append(spare) }
        function addFirst() { rows.insert(0, spare) }
        function look() { seen = rows.get(1).objectName + "," + rows.get(7) + "," + rows.get(-1) }
        Rectangle { id: spare; objectName: "spare"; width: 200; height: 30 }
        ObjectModel {
          id: rows; objectName: "rows"
          Rectangle {
            objectName: "a"; width: 200; height: 20
            property bool mine: ListView.view === list
          }
          Rectangle {
            objectName: "b"; width: 200; height: 40
            property bool inside: parent === list.contentItem
          }
          Rectangle { objectName: "c"; width: 200; height: 60 }
        }
        ListView { id: list; objectName: "list"; anchors.fill: parent; model: rows }
      }
      """
    When 100 milliseconds pass

  Scenario: Each item is where its row is
    Then the property "count" of "list" is 3
    And the property "count" of "rows" is 3
    And the property "contentHeight" of "list" is 120
    And the property "y" of "a" is 0
    And the property "y" of "b" is 20
    And the property "y" of "c" is 60

  Scenario: An item of the model is in the view's content and knows its view
    Then the property "inside" of "b" is true
    And the property "mine" of "a" is true
    And the property "z" of "c" is 1

  Scenario: The first row is the current one
    Then the property "currentIndex" of "list" is 0

  Scenario: An item is added after the others
    When the method "add" of "root" is called
    And 100 milliseconds pass
    Then the property "count" of "rows" is 4
    And the property "count" of "list" is 4
    And the property "y" of "spare" is 120
    And the property "contentHeight" of "list" is 150

  Scenario: An item is put in before the others
    When the method "addFirst" of "root" is called
    And 100 milliseconds pass
    Then the property "y" of "spare" is 0
    And the property "y" of "a" is 30
    And the property "y" of "c" is 90
    And the property "currentIndex" of "list" is 1

  Scenario: An item moves to another row
    When the method "move" of "rows" is called with 0, 2, 1
    And 100 milliseconds pass
    Then the property "y" of "b" is 0
    And the property "y" of "c" is 40
    And the property "y" of "a" is 100
    And the property "count" of "list" is 3

  Scenario: The current row is still the same item after it moved
    When the method "move" of "rows" is called with 0, 2, 1
    And 100 milliseconds pass
    Then the property "currentIndex" of "list" is 2

  Scenario: An item is taken out
    When the method "remove" of "rows" is called with 1, 1
    And 100 milliseconds pass
    Then the property "count" of "rows" is 2
    And the property "count" of "list" is 2
    And the property "y" of "c" is 20
    And the property "contentHeight" of "list" is 80

  Scenario: An item that grows pushes the ones after it
    When the property "height" of "a" is set to 50
    And 100 milliseconds pass
    Then the property "y" of "b" is 50
    And the property "y" of "c" is 90
    And the property "contentHeight" of "list" is 150

  Scenario: A model that is cleared has no rows
    When the method "clear" of "rows" is called
    And 100 milliseconds pass
    Then the property "count" of "rows" is 0
    And the property "count" of "list" is 0
    And the property "contentHeight" of "list" is 0
    And the property "currentIndex" of "list" is -1

  Scenario: An item is asked for by its row
    When the method "look" of "root" is called
    Then the property "seen" of "root" is "b,null,null"
