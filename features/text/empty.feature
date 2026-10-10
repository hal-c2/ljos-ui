# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#lineCount-prop
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#contentHeight-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A Text without text
  An empty Text is still one line of its font tall, and has no width.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property bool sameHeight: empty.height === full.height
        property bool contentIsHeight: empty.contentHeight === empty.height
        property bool biggerFontIsTaller: big.height > empty.height
        Text { id: empty; objectName: "empty" }
        Text { id: full; objectName: "full"; text: "A" }
        Text { id: big; objectName: "big"; font.pixelSize: 40 }
      }
      """
    When 100 milliseconds pass

  Scenario: It counts as one line
    Then the property "lineCount" of "empty" is 1
    And the property "lineCount" of "full" is 1

  Scenario: It is as tall as a line of text
    Then the property "height" of "empty" is at least 1
    And the property "sameHeight" of "root" is true
    And the property "contentIsHeight" of "root" is true

  Scenario: Its height follows the font
    Then the property "biggerFontIsTaller" of "root" is true

  Scenario: It has no width
    Then the property "width" of "empty" is 0
    And the property "contentWidth" of "empty" is 0
