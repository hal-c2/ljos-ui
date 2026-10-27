# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#visible-prop
# Verified on Qt 6.11.2 (qml6).

@qt-parity
Feature: An item whose own visible changes while an ancestor further up hides it
  An item reads not visible while any ancestor is hidden, not only its
  parent. Its own value can change meanwhile; once the ancestor shows,
  the item reads what its own value says.

  Scenario: An item whose value turned true as its grandparent showed is visible
    Given the QML:
      """qml
      import QtQuick
      Item {
        width: 400; height: 400
        Item {
          id: outer
          objectName: "outer"
          visible: false
          Item {
            Item {
              objectName: "inner"
              visible: outer.visible
            }
          }
        }
      }
      """
    When the property "visible" of "outer" is set to true
    Then the property "visible" of "inner" is true

  Scenario: A text that shows once it has text, its grandparent shown in the same step
    Given the QML:
      """qml
      import QtQuick
      Item {
        width: 400; height: 400
        Item {
          id: label
          objectName: "label"
          visible: false
          Item {
            Text {
              id: text
              objectName: "text"
              visible: text.text.length > 0
              text: label.visible ? "shown" : ""
            }
          }
        }
      }
      """
    When the property "visible" of "label" is set to true
    Then the property "text" of "text" is "shown"
    And the property "visible" of "text" is true
