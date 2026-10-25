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
        id: root; objectName: "root"; width: 300; height: 200
        property color seen: inner.palette.button
        property color made: "black"
        property color tone: "#123456"
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
          objectName: "pane"; y: 100; palette.window: root.tone
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

  Scenario: A control's colour set from a script reaches the items inside
    When the property "palette.window" of "pane" is set to "#00ff00"
    Then the property "palette.window" of "inpane" is "#00ff00"

  Scenario: A control's bound colour reaches the items inside as it changes
    When the property "tone" of "root" is set to "#abcdef"
    Then the property "palette.window" of "inpane" is "#abcdef"

  Rule: Each colour group is inherited too

    Scenario: A plain item's groups are those of the items it is in
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        ApplicationWindow {
          palette.button: "#111111"
          palette.disabled.button: "#222222"
          palette.inactive.window: "#333333"
          Item {
            palette.active.base: "#00ffff"
            Item { palette.disabled.text: "#444444"; Item { objectName: "c" } }
          }
        }
        """
      Then the property "palette.active.button" of "c" is "#111111"
      And the property "palette.inactive.button" of "c" is "#111111"
      And the property "palette.disabled.button" of "c" is "#222222"
      And the property "palette.inactive.window" of "c" is "#333333"
      And the property "palette.active.base" of "c" is "#00ffff"
      And the property "palette.disabled.text" of "c" is "#444444"

    Scenario: A group's colour set from a script reaches the plain items inside
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        ApplicationWindow {
          palette.button: "#111111"
          Item { objectName: "a"; Item { objectName: "c" } }
        }
        """
      When the property "palette.inactive.button" of "a" is set to "#555555"
      Then the property "palette.inactive.button" of "c" is "#555555"
      And the property "palette.active.button" of "c" is "#111111"
