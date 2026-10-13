# Sources:
#   https://doc.qt.io/qt-6/qtqml-typesystem-valuetypes.html
#   https://doc.qt.io/qt-6/qml-point.html
#   https://doc.qt.io/qt-6/qml-rect.html
#   https://doc.qt.io/qt-6/qml-color.html
#   https://doc.qt.io/qt-6/qml-font.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Writing a member of a value type held in a variable
  A point, size, rect, colour or font that a script reads into a variable, constant or not, keeps
  where it was read from. Writing one of its members changes the variable and writes the changed
  value back there. A value the script made itself belongs to the variable alone.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property point spot: Qt.point(1, 2)
        property point other
        property rect area: Qt.rect(1, 2, 3, 4)
        property color tint: "red"
        property size extent: Qt.size(3, 4)
        property var bag: ({ p: Qt.point(1, 2) })
        property string out
        Text { id: label; text: "x" }
        function font() { const f = label.font; f.bold = true; f.pixelSize = 31; out = [f.bold, label.font.bold, label.font.pixelSize].join() }
        function point() { const p = spot; p.x = 5; out = p + "," + spot }
        function seen() { var p = root.spot; p.x = 5; out = (p === spot) + "," + (p === Qt.point(5, 2)) }
        function rect() { const r = area; r.width = 10; const s = extent; s.height = 9; out = r + "," + area + "," + extent }
        function color() { const c = tint; c.g = 1; out = c + "," + tint }
        function steps() { let p = spot; p.x += 2; p.y++; out = p + "," + spot }
        function later() { const p = spot; spot = Qt.point(8, 9); p.x = 5; out = p + "," + spot }
        function inside() { const p = spot; const set = (x) => { p.x = x }; set(4); out = p + "," + spot }
        function member() { const p = bag.p; p.x = 5; out = p + "," + bag.p }
        function own() { const p = Qt.point(1, 2); p.x = 5; spot = p; p.y = 7; out = p + "," + spot }
        function given() { const p = spot; other = p; p.x = 7; out = other + "," + spot }
        function again() { let p = spot; p = Qt.point(3, 3); p.x = 4; out = p + "," + spot }
      }
      """

  Rule: A value read from a property is written back to it

    Scenario: Writing a member of a constant font writes the font of the text
      When the method "font" of "root" is called
      Then the property "out" of "root" is "true,true,31"

    Scenario: Writing a member of a point held in a constant writes the property
      When the method "point" of "root" is called
      Then the property "out" of "root" is "QPointF(5, 2),QPointF(5, 2)"

    Scenario: The variable and the property hold the same value after the write
      When the method "seen" of "root" is called
      Then the property "out" of "root" is "true,true"

    Scenario: A rect and a size are written back
      When the method "rect" of "root" is called
      Then the property "out" of "root" is "QRectF(1, 2, 10, 4),QRectF(1, 2, 10, 4),QSizeF(3, 9)"

    Scenario: A colour is written back
      When the method "color" of "root" is called
      Then the property "out" of "root" is "#ffff00,#ffff00"

    Scenario: Compound assignments and increments are written back
      When the method "steps" of "root" is called
      Then the property "out" of "root" is "QPointF(3, 3),QPointF(3, 3)"

    Scenario: A write starts from what the property holds now
      When the method "later" of "root" is called
      Then the property "out" of "root" is "QPointF(5, 9),QPointF(5, 9)"

    Scenario: A function made inside the script writes back too
      When the method "inside" of "root" is called
      Then the property "out" of "root" is "QPointF(4, 2),QPointF(4, 2)"

    Scenario: A value read from a member of a JavaScript object is written back to it
      When the method "member" of "root" is called
      Then the property "out" of "root" is "QPointF(5, 2),QPointF(5, 2)"

  Rule: A value the script made or assigned is its own

    Scenario: A point the script made is changed in its constant alone
      When the method "own" of "root" is called
      Then the property "out" of "root" is "QPointF(5, 7),QPointF(5, 2)"

    Scenario: A property given the variable's value keeps that value
      When the method "given" of "root" is called
      Then the property "out" of "root" is "QPointF(1, 2),QPointF(7, 2)"

    Scenario: A variable given another value no longer writes the property
      When the method "again" of "root" is called
      Then the property "out" of "root" is "QPointF(4, 3),QPointF(1, 2)"
