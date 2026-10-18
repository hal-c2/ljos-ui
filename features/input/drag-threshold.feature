# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-mousearea.html#drag.threshold-prop
#   https://doc.qt.io/qt-6/qml-qtquick-pointerhandler.html#dragThreshold-prop
#   https://doc.qt.io/qt-6/qstylehints.html#startDragDistance-prop
#   https://doc.qt.io/qt-6/qml-qtquick-flickable.html
# Verified on Qt 6.11.2 (qml6, on the offscreen, minimal and vnc platforms:
# each says 10, Qt's own default; a desktop's platform theme may say less).
# Every move here is one pixel at a time, so that the move that starts a
# drag is known to the pixel.

@qt-parity
Feature: One distance starts every drag
  `Qt.styleHints.startDragDistance` says how far the pointer goes before a
  press becomes a drag. A DragHandler, a MouseArea that drags and a
  Flickable all start from it: once the pointer is further than that from
  the press, on an axis they drag.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 400; height: 400
        property real distance: Qt.styleHints.startDragDistance
        property real handlerThreshold: handler.dragThreshold
        property real areaThreshold: area.drag.threshold
        property bool areaActive: area.drag.active
        Rectangle {
          objectName: "box"
          width: 100; height: 100
          DragHandler { id: handler; objectName: "handler" }
        }
        Rectangle {
          id: card
          objectName: "card"
          x: 150; width: 100; height: 100
          MouseArea { id: area; anchors.fill: parent; drag.target: card }
        }
        Flickable {
          objectName: "f"
          y: 200; width: 200; height: 200
          contentWidth: 200; contentHeight: 2000
        }
      }
      """

  @api-QtQuick.PointerHandler.dragThreshold @api-QtQuick.MouseArea.drag.threshold
  Scenario: The system's distance is everybody's threshold
    Then the property "distance" of "root" is 10
    And the property "handlerThreshold" of "root" is 10
    And the property "areaThreshold" of "root" is 10

  @api-QtQuick.DragHandler.active
  Scenario: A DragHandler starts at the first move past the distance
    When the user presses the pointer at 50, 50
    And the user moves the pointer to 50, 49
    And the user moves the pointer to 50, 46
    And the user moves the pointer to 50, 43
    And the user moves the pointer to 50, 41
    And the user moves the pointer to 50, 40
    Then the property "active" of "handler" is false
    When the user moves the pointer to 50, 39
    Then the property "active" of "handler" is true
    And the property "y" of "box" is -11

  @api-QtQuick.MouseArea.drag.active
  Scenario: A MouseArea starts at the move after the first past the distance
    When the user presses the pointer at 200, 50
    And the user moves the pointer to 200, 46
    And the user moves the pointer to 200, 42
    And the user moves the pointer to 200, 40
    And the user moves the pointer to 200, 39
    Then the property "areaActive" of "root" is false
    And the property "y" of "card" is 0
    When the user moves the pointer to 200, 38
    Then the property "areaActive" of "root" is true
    And the property "y" of "card" is -1

  @api-QtQuick.Flickable.dragging
  Scenario: A Flickable starts at the move after the first past the distance
    When the user presses the pointer at 100, 300
    And the user moves the pointer to 100, 296
    And the user moves the pointer to 100, 292
    And the user moves the pointer to 100, 290
    And the user moves the pointer to 100, 289
    Then the property "dragging" of "f" is false
    When the user moves the pointer to 100, 288
    Then the property "dragging" of "f" is true
    And the property "contentY" of "f" is 0
