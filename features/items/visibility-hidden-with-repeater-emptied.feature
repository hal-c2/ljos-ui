# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#visible-prop
#   https://doc.qt.io/qt-6/qml-qtquick-repeater.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-popup.html#close-method
#   https://doc.qt.io/qt-6/qml-qtquick-layouts-stacklayout.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: An item hidden in the same change that removes its Repeater's delegates
  One change can hide an item and take away the delegates of a Repeater
  inside it. The write returns: the item reads hidden and the Repeater
  has no delegates. It does whatever hides the item: a property, a popup
  that closes, a page a layout stops showing.

  Scenario: Hiding an item whose Repeater empties in the same change returns
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property bool shown: true
        Item {
          objectName: "card"
          visible: root.shown
          Repeater {
            objectName: "repeater"
            model: root.shown ? 1 : 0
            delegate: Item {
              Item {}
              Item {}
            }
          }
        }
      }
      """
    When the property "shown" of "root" is set to false
    Then the property "visible" of "card" is false
    And the property "count" of "repeater" is 0

  Scenario: Closing a popup whose Repeater empties as it closes returns
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        width: 400; height: 400
        Popup {
          id: popup
          objectName: "popup"
          Repeater {
            objectName: "repeater"
            model: popup.contentItem.visible ? 1 : 0
            delegate: Item {
              Item {}
              Item {}
            }
          }
        }
      }
      """
    When the method "open" of "popup" is called
    And the method "close" of "popup" is called
    Then the property "visible" of "popup" is false
    And the property "count" of "repeater" is 0

  Scenario: Switching away from a page whose Repeater empties as it goes returns
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Layouts
      StackLayout {
        id: stack
        objectName: "stack"
        width: 400; height: 400
        currentIndex: 0
        Item {
          id: first
          objectName: "first"
          Repeater {
            objectName: "repeater"
            model: first.visible ? 1 : 0
            delegate: Item {
              Item {}
              Item {}
            }
          }
        }
        Item { objectName: "second" }
      }
      """
    When the property "currentIndex" of "stack" is set to 1
    Then the property "visible" of "first" is false
    And the property "count" of "repeater" is 0
