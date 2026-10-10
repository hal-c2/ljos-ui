# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-repeater.html
#   https://doc.qt.io/qt-6/qtquick-modelviewsdata-modelview.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Repeater
  A Repeater creates one delegate per row of its model, as children of its own parent.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Row {
        id: root
        objectName: "root"
        property string picked: "?"
        property string log: ""
        function pick(i) {
          var it = rep.itemAt(i)
          picked = it ? it.objectName : "none"
        }
        Item { objectName: "before"; width: 5; height: 5 }
        Repeater {
          id: rep
          objectName: "rep"
          model: 3
          onItemAdded: (i, item) => root.log += "+" + i + item.objectName
          onItemRemoved: (i, item) => root.log += "-" + i
          Item { objectName: "d" + index; width: 10; height: 10 }
        }
        Item { objectName: "after"; width: 5; height: 5 }
      }
      """
    When 100 milliseconds pass

  Scenario: One delegate per row
    Then the property "count" of "rep" is 3
    And the object "d0" exists
    And the object "d2" exists
    And the object "d3" does not exist

  Scenario: The delegates are placed before the Repeater among its siblings
    Then the property "x" of "before" is 0
    And the property "x" of "d0" is 5
    And the property "x" of "d1" is 15
    And the property "x" of "d2" is 25
    And the property "x" of "after" is 35
    And the property "width" of "root" is 40

  Scenario: The Repeater itself takes no room
    Then the property "width" of "rep" is 0
    And the property "height" of "rep" is 0

  Scenario: itemAdded is emitted for each delegate, in order
    Then the property "log" of "root" is "+0d0+1d1+2d2"

  Scenario Outline: itemAt returns the delegate of a row, or nothing outside the model
    When the method "pick" of "root" is called with <row>
    Then the property "picked" of "root" is "<name>"

    Examples:
      | row | name |
      | 0   | d0   |
      | 2   | d2   |
      | 3   | none |
      | -1  | none |

  Scenario: A larger number model creates the delegates again
    When the property "model" of "rep" is set to 4
    And 100 milliseconds pass
    Then the property "count" of "rep" is 4
    And the object "d3" exists
    And the property "x" of "after" is 45
    And the property "log" of "root" is "+0d0+1d1+2d2-2-1-0+0d0+1d1+2d2+3d3"

  Scenario: A model of zero rows removes every delegate
    When the property "model" of "rep" is set to 0
    And 100 milliseconds pass
    Then the property "count" of "rep" is 0
    And the object "d0" does not exist
    And the property "x" of "after" is 5
