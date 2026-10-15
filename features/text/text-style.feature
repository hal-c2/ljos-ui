# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#style-prop
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#styleColor-prop
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#font.styleName-prop
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#font.kerning-prop
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#font.overline-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Text style and the rest of the font
  Text.style draws copies of the text in styleColor around or beside it and
  never changes the size of the Text. The font value type keeps every member
  it is given.

  Scenario: The style and its colour are kept and the size does not change
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property bool sameSize: raised.width === plain.width && raised.height === plain.height
        Text { id: plain; text: "Hello" }
        Text { id: raised; objectName: "raised"; text: "Hello"; style: Text.Raised; styleColor: "#ff0000" }
      }
      """
    Then the property "style" of "raised" is 2
    And the property "styleColor" of "raised" is "#ff0000"
    And the property "sameSize" of "root" is true

  Scenario: Font members are stored and read back
    Given the QML:
      """qml
      import QtQuick
      Text {
        objectName: "t"
        text: "x"
        font.styleName: "Bold"
        font.kerning: false
        font.preferShaping: false
        font.overline: true
        font.contextFontMerging: true
        font.preferTypoLineMetrics: true
      }
      """
    Then the property "font.styleName" of "t" is "Bold"
    And the property "font.kerning" of "t" is false
    And the property "font.preferShaping" of "t" is false
    And the property "font.overline" of "t" is true
    And the property "font.contextFontMerging" of "t" is true
    And the property "font.preferTypoLineMetrics" of "t" is true
