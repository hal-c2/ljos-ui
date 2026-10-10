# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-fontmetrics.html
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#contentWidth-prop
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#elide-prop
# The numbers are the ones Qt 6.11.2 (qml6, offscreen) gives for Liberation
# Sans at the same pixel sizes. Qt takes its default font from the system, so
# only Ljós, which carries the font, can be held to them.

@ljos-only
Feature: Exact text numbers with the bundled font
  Without system fonts Ljós sets text in the font it carries, and then every
  number is known: text measures the same on every machine.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property real advance: fm.advanceWidth("Hello world")
        property real kerned: fm.advanceWidth("AV")
        property rect box: fm.boundingRect("Hello world")
        property string atEnd: fm.elidedText("Hello world", Qt.ElideRight, 40)
        property string atStart: fm.elidedText("Hello world", Qt.ElideLeft, 40)
        property string inMiddle: fm.elidedText("Hello world", Qt.ElideMiddle, 40)
        FontMetrics { id: fm; objectName: "fm"; font.pixelSize: 14 }
        Text { objectName: "line"; text: "Hello world"; font.pixelSize: 14 }
        Text { objectName: "small"; text: "Hello"; font.pixelSize: 8 }
        Text { objectName: "large"; text: "Hello"; font.pixelSize: 48 }
        Text { objectName: "points"; text: "Hello world"; font.pointSize: 11 }
      }
      """
    When 100 milliseconds pass

  Scenario: The numbers of the font at 14 pixels
    Then the property "ascent" of "fm" is 12.671875
    And the property "descent" of "fm" is 2.953125
    And the property "height" of "fm" is 15.625
    And the property "leading" of "fm" is 0
    And the property "xHeight" of "fm" is 7.390625
    And the property "averageCharacterWidth" of "fm" is 8.109375
    And the property "maximumCharacterWidth" of "fm" is 19
    And the property "underlinePosition" of "fm" is 0.453125
    And the property "lineWidth" of "fm" is 1.03125

  Scenario: Advances are counted in 64ths of a pixel
    Then the property "advance" of "root" is 69.203125
    And the property "kerned" of "root" is 17.625
    And the property "box.width" of "root" is 68
    And the property "box.height" of "root" is 15.625

  Scenario: A line of text is as wide as its advance and a whole number of pixels tall
    Then the property "width" of "line" is 69.203125
    And the property "height" of "line" is 16
    And the property "baselineOffset" of "line" is 12.671875

  Scenario: The line height follows the pixel size
    Then the property "height" of "small" is 9
    And the property "height" of "large" is 54

  Scenario: Eleven points are set at fifteen pixels
    Then the property "width" of "points" is 74.09375
    And the property "height" of "points" is 17

  Scenario: Eliding cuts at the same character as Qt
    Then the property "atEnd" of "root" is "Hell…"
    And the property "atStart" of "root" is "…orld"
    And the property "inMiddle" of "root" is "H…d"
