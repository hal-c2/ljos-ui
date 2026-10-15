# Sources:
#   https://doc.qt.io/qt-6/qtquick-modelviewsdata-modelview.html#javascript-arrays-and-objects
#   https://doc.qt.io/qt-6/qtquick-modelviewsdata-modelview.html#required-properties
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: An array of objects as a model, with required properties
  A delegate of a JavaScript array of objects is given the members of
  its row through required properties of the same name. The names are
  those of the first row and of the row itself; a row that lacks one of
  the first row's names gives the property its default. Writing such a
  property changes the delegate only, not the array.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Column {
        id: root
        objectName: "root"
        property var days: [ { day: 5, on: true }, { day: 6 }, 7, { other: 1 } ]
        property bool firstOn: false
        function read() { firstOn = days[0].on }
        Repeater {
          model: root.days
          Item {
            required property int index
            required property int day
            required property bool on
            objectName: "d" + index
            function flip() { on = !on }
          }
        }
      }
      """

  Scenario Outline: A required property is filled from the row's member
    Then the property "<property>" of "<row>" is <value>

    Examples:
      | row | property | value |
      | d0  | day      | 5     |
      | d0  | on       | true  |
      | d1  | day      | 6     |
      | d1  | on       | false |
      | d2  | day      | 0     |
      | d3  | day      | 0     |

  Scenario: Every row of the array has a delegate
    Then the log has no warnings

  Scenario: Writing a required property changes the delegate, not the array
    When the method "flip" of "d0" is called
    And the method "read" of "root" is called
    Then the property "on" of "d0" is false
    And the property "firstOn" of "root" is true

  Scenario: A name only a later row has is required of that row alone
    Given the QML:
      """qml
      import QtQuick
      Column {
        Repeater {
          model: [ { day: 1 }, { other: 2 } ]
          Item { required property var other; objectName: "o" + other }
        }
      }
      """
    Then the object "o2" exists
    And the log has a warning matching "Required property other was not initialized"
