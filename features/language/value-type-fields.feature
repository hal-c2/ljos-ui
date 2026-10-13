# Sources:
#   https://doc.qt.io/qt-6/qtqml-typesystem-valuetypes.html
#   https://doc.qt.io/qt-6/qtqml-syntax-objectattributes.html#grouped-properties
#   https://doc.qt.io/qt-6/qtqml-syntax-objectattributes.html#attached-properties-and-attached-signal-handlers
#   https://doc.qt.io/qt-6/qml-qtquick-drag.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Assigning a field of a value-type property
  A declaration may give one field of a value-type property instead of the whole value:
  `font.bold: true`, `spot.x: 10`. The field may belong to a property of the object itself,
  to a property of a grouped object (`border.width`) or to a property of an attached object
  (`Drag.hotSpot.x`). Each is written in dotted form or in a group block, as a literal or as a
  binding, and the fields not given keep their defaults.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property real k: 3
        property point spot
        spot.x: 10
        spot.y: 4
        property rect area
        area.width: 30
        Text { id: label; text: "x"; font.bold: true; font.pixelSize: 21 }
        Text { id: block; text: "x"; font { bold: true; pixelSize: 19 } }
        Text { id: bound; text: "x"; font.pixelSize: root.k * 5 }
        Rectangle { id: border; border.width: 3; border.color: "red" }
        Item { id: margins; anchors.margins: 5 }
        Item { id: hot; Drag.hotSpot.x: 10; Drag.hotSpot.y: 3 }
        Item { id: hotBlock; Drag.hotSpot { x: 6; y: 8 } }
        Item { id: hotBound; Drag.hotSpot.x: root.k * 2 }
        function own() { return spot + "," + area }
        function font() { return [label.font.bold, label.font.pixelSize, block.font.bold, block.font.pixelSize].join() }
        function grouped() { return border.border.width + "," + border.border.color + "," + margins.anchors.margins }
        function attached() { return hot.Drag.hotSpot + "," + hotBlock.Drag.hotSpot }
        function followed() { return bound.font.pixelSize + "," + hotBound.Drag.hotSpot }
        property string out
      }
      """

  Rule: A field is written into the value of the property

    Scenario: Fields of the object's own point and rect
      When the property "out" of "root" is bound to "own()"
      Then the property "out" of "root" is "QPointF(10, 4),QRectF(0, 0, 30, 0)"

    Scenario: Fields of a font, dotted and in a block
      When the property "out" of "root" is bound to "font()"
      Then the property "out" of "root" is "true,21,true,19"

    Scenario: Properties of a grouped object
      When the property "out" of "root" is bound to "grouped()"
      Then the property "out" of "root" is "3,#ff0000,5"

    Scenario: Fields of a point on an attached object, dotted and in a block
      When the property "out" of "root" is bound to "attached()"
      Then the property "out" of "root" is "QPointF(10, 3),QPointF(6, 8)"

  Rule: A field given by a binding follows it

    Scenario: A font field and an attached point field follow what they are bound to
      When the property "out" of "root" is bound to "followed()"
      Then the property "out" of "root" is "15,QPointF(6, 0)"
      When the property "k" of "root" is set to 4
      Then the property "out" of "root" is "20,QPointF(8, 0)"
