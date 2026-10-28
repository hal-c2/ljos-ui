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
        R { y: 200; text: "<span style=\"text-decoration: underline; text-decoration-color: #00ff00\">" + gap + "</span>" }
        R { y: 300; text: "<span style=\"text-decoration: line-through; text-decoration-color: #ff0000\">" + gap + "</span>" }
        R { x: 200; text: "<span style=\"text-decoration: overline; text-decoration-color: #ff0000\">" + gap + "</span>" }
      }
      """
    When 100 milliseconds pass

  Scenario: An underlined element paints its line in the element's text colour
    Then the pixel at 20, 59 of the window is within 5 of "#00ff00"

  @qt-parity
  Scenario: A struck-through element paints a line through its text
    Then the pixel at 20, 140 of the window is within 5 of "#000000"

  Scenario: A span paints its underline in the decoration colour
    Then the pixel at 20, 259 of the window is within 5 of "#00ff00"

  @qt-parity
  Scenario: A span paints its strike-through in the decoration colour
    Then the pixel at 20, 340 of the window is within 5 of "#ff0000"

  @qt-parity
  Scenario: A span paints its overline in the decoration colour
    Then the pixel at 220, 1 of the window is within 5 of "#ff0000"
