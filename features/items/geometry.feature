# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-item.html
#   https://doc.qt.io/qt-6/qml-qtquick-rectangle.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Item geometry and colour
  Geometry properties are plain properties that bindings can read.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        width: 200; height: 100
        Rectangle {
          objectName: "box"
          x: 10; y: 20
          width: parent.width / 2
          height: 30
          color: "#ff0000"
        }
      }
      """

  Scenario: A child's width is bound to its parent
    Then the property "width" of "box" is 100

  Scenario: A child follows its parent when the parent resizes
    When the property "width" of "root" is set to 400
    Then the property "width" of "box" is 200

  Scenario Outline: Rectangle reports its colour as hex
    When the property "color" of "box" is set to <input>
    Then the property "color" of "box" is <hex>

    Examples:
      | input     | hex       |
      | "#00ff00" | "#00ff00" |
      | "#0000FF" | "#0000ff" |

  Scenario: A new item is visible by default
    Then the property "visible" of "box" is true

  Scenario: An item can be hidden and shown again
    When the property "visible" of "box" is set to false
    Then the property "visible" of "box" is false
    When the property "visible" of "box" is set to true
    Then the property "visible" of "box" is true

  Scenario: Setting a property that does not exist is rejected
    Then setting the property "nothing" of "box" to 1 is rejected
