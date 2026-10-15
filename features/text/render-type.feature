# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#renderType-prop
#   https://doc.qt.io/qt-6/qml-qtquick-textinput.html#renderType-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Render type
  renderType picks how glyphs are drawn. It is a property of every text item and of the text
  controls built on them, in every style, and a control passes its own on to the text it holds.

  Scenario Outline: A text item takes a render type and keeps it
    Given the QML:
      """qml
      import QtQuick
      Item {
        Text { objectName: "t"; text: "a"; renderType: Text.<type> }
      }
      """
    Then the property "renderType" of "t" is <value>

    Examples:
      | type            | value |
      | QtRendering     | 0     |
      | NativeRendering | 1     |
      | CurveRendering  | 2     |

  Scenario: Text draws with Qt rendering unless told otherwise
    Given the QML:
      """qml
      import QtQuick
      Text { objectName: "t"; text: "a" }
      """
    Then the property "renderType" of "t" is 0

  Scenario Outline: A text control takes a render type in a style
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      import QtQuick.Controls.<style>
      Column {
        TextField { objectName: "field"; renderType: Text.NativeRendering; placeholderText: "p" }
        TextArea { objectName: "area"; renderType: Text.CurveRendering }
        Label { objectName: "label"; text: "a"; renderType: Text.NativeRendering }
      }
      """
    Then the property "renderType" of "field" is 1
    And the property "renderType" of "area" is 2
    And the property "renderType" of "label" is 1

    Examples:
      | style     |
      | Basic     |
      | Material  |
      | Fusion    |
      | Universal |
