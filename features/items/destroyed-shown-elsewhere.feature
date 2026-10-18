# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#children-prop
#   https://doc.qt.io/qt-6/qtqml-javascript-dynamicobjectcreation.html#deleting-objects-dynamically
#   https://doc.qt.io/qt-6/qml-qtquick-controls-overlay.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: An object destroyed with its owner leaves the item it was shown in
  An object goes with the one it is written in, also when it was shown in
  another item. That item does not list it afterwards.

  Scenario: An item shown in another leaves its children when its owner is destroyed
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        Item { id: a; objectName: "a"; property int n: children.length }
        Item { id: owner; Item { parent: a; width: 5 } }
        function kill() { owner.destroy() }
      }
      """
    When 100 milliseconds pass
    When the method "kill" of "root" is called
    And 100 milliseconds pass
    Then the property "n" of "a" is 0

  Scenario: A destroyed popup leaves the overlay
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      ApplicationWindow {
        objectName: "root"; width: 200; height: 200; visible: true
        property int n: Overlay.overlay ? Overlay.overlay.children.length : -1
        Dialog { id: d; width: 50; height: 50 }
        function kill() { d.destroy() }
      }
      """
    When 100 milliseconds pass
    When the method "kill" of "root" is called
    And 100 milliseconds pass
    Then the property "n" of "root" is 0
