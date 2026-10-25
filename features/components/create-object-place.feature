# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-component.html#createObject-method
#   https://doc.qt.io/qt-6/qtqml-javascript-dynamicobjectcreation.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Where an object made under a parent goes
  `createObject(parent)` makes `parent` the new object's owner. An item made
  under an item is also its visual child, one of its own `children`, whatever
  the parent's default property is; nothing else made under a parent goes in
  any property of it.

  Scenario: An item made under a component with a default property alias is the component's own child
    Given the file "Card.qml":
      """qml
      import QtQuick
      Item {
        objectName: "card"
        default property alias content: body.data
        Item { id: body; objectName: "body" }
      }
      """
    And the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string got
        Card { id: card }
        Component { id: made; Item { } }
        Component.onCompleted: {
          let item = made.createObject(card)
          got = item.parent.objectName + " " + card.children.length + " " + card.content.length
        }
      }
      """
    Then the property "got" of "root" is "card 2 0"

  Scenario: An item made under a component whose default property is a Component is its child
    Given the file "Slot.qml":
      """qml
      import QtQuick
      Item { objectName: "slot"; default property Component delegate }
      """
    And the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string got
        Slot { id: slot; Text { text: "in the slot" } }
        Component.onCompleted: {
          let text = slot.delegate.createObject(slot)
          got = text.text + " " + text.parent.objectName
        }
      }
      """
    Then the property "got" of "root" is "in the slot slot"
    And the log has no warnings

  Scenario: An object that is not an item made under an item is in none of its lists
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string got
        Item { id: host }
        Component { id: made; Timer { } }
        Component.onCompleted: {
          let timer = made.createObject(host)
          got = (timer !== null) + " " + host.data.length + " " + host.resources.length
        }
      }
      """
    Then the property "got" of "root" is "true 0 0"
