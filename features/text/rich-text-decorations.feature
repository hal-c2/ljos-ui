# Sources:
#   https://doc.qt.io/qt-6/richtext-html-subset.html
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#textFormat-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

Feature: Decorations and backgrounds in rich text
  RichText draws the lines named by its elements and styles, and paints an
  inline element's background behind its glyphs.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        width: 400; height: 400; color: "white"
        property string gap: "&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;"
        component R: Text { textFormat: Text.RichText; font.pixelSize: 60 }
        R { text: "<u style=\"color: #00ff00\">" + gap + "</u>" }
        R { y: 100; text: "<s>" + gap + "</s>" }
      }
      """
    When 100 milliseconds pass

  Scenario: An underlined element paints its line in the element's text colour
    Then the pixel at 20, 59 of the window is within 5 of "#00ff00"

  @qt-parity
  Scenario: A struck-through element paints a line through its text
    Then the pixel at 20, 140 of the window is within 5 of "#000000"
