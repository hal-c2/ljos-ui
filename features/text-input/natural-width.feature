# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-textinput.html#contentWidth-prop
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#implicitWidth-prop
# Verified on Qt 6.11.2 (qmltestrunner and the oracle, offscreen).

@qt-parity
Feature: A TextInput given no width is as wide as its text
  A TextInput's implicitWidth is its text's width rounded up to a whole
  pixel, plus its padding. Given no width, though, the item is as wide as
  the text is, to the fraction of a pixel, plus the padding: a Row puts
  the next item right after the text. The numbers are those of the
  bundled font, Liberation Sans, at 14 pixels.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        width: 300; height: 200
        Row {
          objectName: "row"
          TextInput {
            objectName: "input"
            font.family: "Liberation Sans"; font.pixelSize: 14
            text: "hello brave world"
          }
          Rectangle { objectName: "after"; width: 5; height: 5 }
        }
        TextInput {
          objectName: "padded"; y: 50; leftPadding: 3; rightPadding: 2
          font.family: "Liberation Sans"; font.pixelSize: 14
          text: "hello brave world"
        }
      }
      """

  @api-QtQuick.TextInput.contentWidth
  Scenario: The item is as wide as the text, and the next item follows it
    Then the property "width" of "input" is 105.75
    And the property "x" of "after" is 105.75
    And the property "width" of "padded" is 110.75

  Scenario: A new text gives a new width, still to the fraction
    When the property "text" of "input" is set to "hello"
    Then the property "width" of "input" is 29.5625
    When 100 milliseconds pass
    Then the property "x" of "after" is 29.5625

  @api-QtQuick.Item.implicitWidth
  Scenario: The implicit width is rounded up
    Then the property "implicitWidth" of "input" is 106
    And the property "implicitWidth" of "padded" is 111
