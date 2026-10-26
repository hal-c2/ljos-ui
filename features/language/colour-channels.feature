# Sources:
#   https://doc.qt.io/qt-6/qml-color.html
#   https://doc.qt.io/qt-6/qtqml-typesystem-valuetypes.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Assigning a channel of a colour property
  A colour's channels `r`, `g`, `b` and `a` are fields of the value, so a declaration may give
  one of them instead of the whole colour: `tint.g: 0.5`, `color.r: 0.5`. A colour that had no
  value starts as opaque black, and the channels not given keep that.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property real k: 3
        property color tint
        tint.g: root.k / 6
        tint.a: 0.25
        Rectangle { id: fill; color.r: 0.5 }
        function colours() { return tint + "," + fill.color }
        property string out
      }
      """

  Rule: A channel is written into the colour

    Scenario: Channels of the object's own colour and of another object's
      When the property "out" of "root" is bound to "colours()"
      Then the property "out" of "root" is "#40008000,#80ffff"

  Rule: A channel given by a binding follows it

    Scenario: A channel of a colour follows what it is bound to
      When the property "out" of "root" is bound to "colours()"
      And the property "k" of "root" is set to 4
      Then the property "out" of "root" is "#4000aa00,#80ffff"
