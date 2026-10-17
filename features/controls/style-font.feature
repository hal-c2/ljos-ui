# Sources:
#   https://doc.qt.io/qt-6/qtquickcontrols-material.html
#   https://doc.qt.io/qt-6/qtquickcontrols-universal.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-control.html#font-prop
# Verified on Qt 6.11.2 (qmltestrunner, offscreen).

@qt-parity @api-QtQuick.Controls.Control.font
Feature: The font a style gives its controls
  Material and Universal each have a font of their own, a little larger
  than Basic's, and some controls take another size or weight of it. An
  attribute set on a control above is handed down over the style's.

  Scenario: Material's controls are 14 pixels, their buttons medium weight
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Material
      Pane {
        objectName: "pane"
        Column {
          Label { objectName: "label"; text: "Label" }
          Button { objectName: "button"; text: "B" }
          ToolButton { objectName: "tool"; text: "T" }
          ItemDelegate { objectName: "delegate"; text: "D" }
          CheckDelegate { objectName: "check"; text: "C" }
          TextField { objectName: "field" }
          MenuItem { objectName: "item"; text: "M" }
          Frame { objectName: "frame" }
        }
      }
      """
    Then the property "font.pixelSize" of "pane" is 14
    And the property "font.weight" of "pane" is 400
    And the property "font.pixelSize" of "label" is 14
    And the property "implicitHeight" of "label" is 16
    And the property "font.weight" of "button" is 500
    And the property "font.weight" of "tool" is 500
    And the property "font.weight" of "delegate" is 500
    And the property "font.pixelSize" of "check" is 16
    And the property "font.weight" of "check" is 400
    And the property "font.pixelSize" of "field" is 16
    And the property "font.pixelSize" of "item" is 16
    And the property "font.pixelSize" of "frame" is 14

  Scenario: An attribute set above is handed down over the style's
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Material
      Column {
        Pane {
          font.pixelSize: 20
          Button { objectName: "big"; text: "B" }
        }
        Pane {
          font.weight: 300
          Column {
            Button { objectName: "light"; text: "B" }
            TextField { objectName: "field" }
          }
        }
      }
      """
    Then the property "font.pixelSize" of "big" is 20
    And the property "font.weight" of "big" is 500
    And the property "font.weight" of "light" is 300
    And the property "font.pixelSize" of "field" is 16
    And the property "font.weight" of "field" is 300

  Scenario: Universal's controls are 15 pixels, its tabs 24 and light
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Universal
      Pane {
        objectName: "pane"
        Column {
          Label { objectName: "label"; text: "Label" }
          Button { objectName: "button"; text: "B" }
          TabButton { objectName: "tab"; text: "T" }
        }
      }
      """
    Then the property "font.pixelSize" of "pane" is 15
    And the property "implicitHeight" of "label" is 17
    And the property "font.pixelSize" of "button" is 15
    And the property "font.weight" of "button" is 400
    And the property "font.pixelSize" of "tab" is 24
    And the property "font.weight" of "tab" is 300

  Scenario: Basic's controls keep 12 pixels
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Pane {
        Button { objectName: "button"; text: "B" }
      }
      """
    Then the property "font.pixelSize" of "button" is 12
    And the property "font.weight" of "button" is 400
