# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#palette-prop
# Verified on Qt 6.11.2 (qml6, offscreen).
#
# What a plain item reads where nobody above it set a colour is the
# platform's on qml6, so every colour here is set above.

@qt-parity @api-QtQuick.Item.palette
Feature: A plain item's colours are those of the items it is in
  A plain item that sets no colour of a role has the nearest ancestor's.
  A binding on it follows when that changes, an item made later inside
  has it, and a colour the item sets itself stays its own.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        id: root
        objectName: "root"
        width: 300; height: 200
        property color seen: inner.palette.button
        property color made: "black"
        function make() { made = later.createObject(inner).palette.button }
        Component { id: later; Item {} }
        Item {
          objectName: "outer"
          palette { button: "red"; highlight: "green" }
          Item {
            id: inner
            objectName: "inner"
            palette.highlight: "blue"
            Rectangle { objectName: "rect"; width: 10; height: 10 }
          }
        }
        Pane {
          objectName: "pane"; y: 100; palette.window: "#123456"
          Item { objectName: "inpane" }
        }
      }
      """

  Scenario: A binding on a plain item's colour follows the item above
    Then the property "seen" of "root" is "#ff0000"
    When the property "palette.button" of "outer" is set to "#0000ff"
    Then the property "seen" of "root" is "#0000ff"

  Scenario: A colour the item set stays, and the others come from above
    Then the property "palette.highlight" of "inner" is "#0000ff"
    And the property "palette.button" of "inner" is "#ff0000"
    And the property "palette.highlight" of "rect" is "#0000ff"
    And the property "palette.button" of "rect" is "#ff0000"

  Scenario: An item made later inside has the colours
    When the property "palette.button" of "outer" is set to "#00ff00"
    And the method "make" of "root" is called
    Then the property "made" of "root" is "#00ff00"

  Scenario: An item inside a control has the control's colours
    Then the property "palette.window" of "inpane" is "#123456"
