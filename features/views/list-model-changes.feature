# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-listview.html
#   https://doc.qt.io/qt-6/qml-qtqml-models-listmodel.html
#   docs/ARCHITECTURE.md section 8.4 (the scroll anchor is a row, not a pixel offset)
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A scrolled ListView keeps what it shows when its model changes
  Rows added or removed above the view do not move the rows the user is looking at.

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
          Component.onCompleted: { for (var i = 0; i < 200; ++i) append({ name: "p" + i }) }
          function prepend(n) { for (var i = 0; i < n; ++i) insert(0, { name: "new" + i }) }
        }
        property int first: -2
        function look() { first = indexAt(10, contentY + 1) }
        delegate: Item {
          required property string name
          objectName: name; width: 200; height: 20
        }
      }
      """
    When the property "contentY" of "list" is set to 1000
    And 100 milliseconds pass

  Scenario: The list is scrolled to its middle
    When the method "look" of "list" is called
    Then the property "first" of "list" is 50
    And the property "y" of "p50" is 1000

  Scenario: Fifty rows prepended above the view move nothing the user sees
    When the method "prepend" of "lm" is called with 50
    And 100 milliseconds pass
    Then the property "count" of "list" is 250
    And the property "contentY" of "list" is 1000
    And the property "y" of "p50" is 1000
    And the property "y" of "p55" is 1100
    And the property "originY" of "list" is -1000
    And the property "contentHeight" of "list" is 5000
    When the method "look" of "list" is called
    Then the property "first" of "list" is 100

  Scenario: The current row is still the same row after rows were prepended
    When the method "prepend" of "lm" is called with 50
    And 100 milliseconds pass
    Then the property "currentIndex" of "list" is 50
    And the property "currentItem.objectName" of "list" is "p0"

  Scenario: Rows removed above the view move nothing the user sees
    When the method "remove" of "lm" is called with 0, 30
    And 100 milliseconds pass
    Then the property "count" of "list" is 170
    And the property "contentY" of "list" is 1000
    And the property "y" of "p50" is 1000
    And the property "originY" of "list" is 600
    And the property "contentHeight" of "list" is 3400
    When the method "look" of "list" is called
    Then the property "first" of "list" is 20

  Scenario: A row inserted inside the view pushes the later rows down
    When the method "insert" of "lm" is called with 52, {"name": "mid"}
    And 100 milliseconds pass
    Then the property "y" of "p51" is 1020
    And the property "y" of "mid" is 1040
    And the property "y" of "p52" is 1060
    And the property "contentY" of "list" is 1000

  Scenario: A row removed inside the view pulls the later rows up
    When the method "remove" of "lm" is called with 52
    And 100 milliseconds pass
    Then the object "p52" does not exist
    And the property "y" of "p53" is 1040
    And the property "contentY" of "list" is 1000

  Scenario: Rows appended below the view only make the list longer
    When the method "append" of "lm" is called with {"name": "last"}
    And 100 milliseconds pass
    Then the property "contentY" of "list" is 1000
    And the property "y" of "p50" is 1000
    And the property "contentHeight" of "list" is 4020

  Scenario: A row moved inside the view changes places with its neighbours
    When the method "move" of "lm" is called with 51, 53, 1
    And 100 milliseconds pass
    Then the property "y" of "p52" is 1020
    And the property "y" of "p53" is 1040
    And the property "y" of "p51" is 1060

  Scenario: Clearing the model empties the view
    When the method "clear" of "lm" is called
    And 100 milliseconds pass
    Then the property "count" of "list" is 0
    And the property "contentHeight" of "list" is 0
    And the property "contentY" of "list" is 0
    And the property "originY" of "list" is 0
    And the object "p50" does not exist
