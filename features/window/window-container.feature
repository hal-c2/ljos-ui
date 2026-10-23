# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-windowcontainer.html
#   https://doc.qt.io/qt-6/qml-qtquick-window.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A window shown in a WindowContainer
  A WindowContainer is an item that shows a window in its place. The window
  becomes part of the window the container is in: it is shown, it belongs
  to no other window, and it is as large as the container. A container
  that was given no size is as large as its window, and one without a
  window is an empty item.

  @api-QtQuick.WindowContainer.window
  Scenario: A container without a window is an empty item
    Given the QML:
      """qml
      import QtQuick
      Window {
        objectName: "root"
        width: 200; height: 100; visible: true
        property bool empty: box.window === null
        property real side: box.implicitWidth + box.implicitHeight
        WindowContainer { id: box }
      }
      """
    Then the property "empty" of "root" is true
    And the property "side" of "root" is 0

  @api-QtQuick.WindowContainer.window @api-QtQuick.Window.visible
  Scenario: A window put in a container is shown and belongs to no other window
    Given the QML:
      """qml
      import QtQuick
      Window {
        objectName: "root"
        width: 200; height: 100; visible: true
        property bool inner: shown.transientParent === null
        Window { id: shown; objectName: "shown"; width: 30; height: 25 }
        WindowContainer { window: shown }
      }
      """
    When 50 milliseconds pass
    Then the property "visible" of "shown" is true
    And the property "inner" of "root" is true

  @api-QtQuick.WindowContainer.window @api-QtQuick.Item.implicitWidth
  Scenario: A container given no size is as large as its window
    Given the QML:
      """qml
      import QtQuick
      Window {
        width: 200; height: 100; visible: true
        Window { id: shown; width: 30; height: 25 }
        WindowContainer { objectName: "box"; window: shown }
      }
      """
    When 50 milliseconds pass
    Then the property "width" of "box" is 30
    And the property "height" of "box" is 25

  @api-QtQuick.WindowContainer.window @api-QtQuick.Window.width
  Scenario: The window is as large as its container
    Given the QML:
      """qml
      import QtQuick
      Window {
        width: 200; height: 100; visible: true
        Window { id: shown; objectName: "shown"; width: 30; height: 25 }
        WindowContainer { x: 10; y: 20; width: 50; height: 40; window: shown }
      }
      """
    When 50 milliseconds pass
    Then the property "width" of "shown" is 50
    And the property "height" of "shown" is 40
