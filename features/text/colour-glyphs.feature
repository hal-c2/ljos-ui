# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#color-prop
#   https://learn.microsoft.com/typography/opentype/spec/cbdt
# Checked against Qt 6.12.0 (qml6, offscreen) on Linux, with the font below
# and with Noto Color Emoji: a colour glyph is drawn in its own colours
# whatever the text's colour is, even transparent or half transparent, and
# fades with the item's opacity. A text style draws its copies as the
# picture too, not in the style's colour: Raised shows the picture's blue
# one row below it.
# LjosColorTest-Regular.ttf (features/text/fonts, made by make-color-test.py
# there) is a colour bitmap font like Noto Color Emoji: its U+F8F1 is a PNG
# of 20 by 20 pixels at 20 pixels per em, red above and blue below, the top
# 16 pixels above the baseline. Its ascent is 16 pixels at 20, so the picture
# starts at the top of the text.

@qt-parity @api-QtQuick.Text.color
Feature: Colour glyphs
  A font such as an emoji font keeps some glyphs as pictures, not outlines.
  Such a glyph is drawn as its picture, scaled to the text's size, in the
  picture's own colours: the text's colour does not tint it. The item's
  opacity fades it as it fades everything the item draws.

  Background:
    Given the font file "LjosColorTest-Regular.ttf"
    And the QML:
      """qml
      import QtQuick
      Rectangle {
        width: 200; height: 80; color: "black"
        FontLoader { id: font; source: "LjosColorTest-Regular.ttf" }
        component G: Text { text: ""; font.family: font.name; font.pixelSize: 20; color: "white" }
        G { x: 10; y: 10 }
        G { x: 50; y: 10; color: "transparent" }
        G { x: 90; y: 10; color: "#8000ff00" }
        G { x: 130; y: 10; font.pixelSize: 40 }
        G { x: 10; y: 50; opacity: 0.5 }
        G { x: 50; y: 50; style: Text.Raised; styleColor: "lime" }
      }
      """
    When 100 milliseconds pass

  Scenario: A colour glyph is drawn in its picture's colours
    Then the pixel at 19, 12 of the window is "#ff0000"
    And the pixel at 19, 27 of the window is "#0000ff"

  Scenario: The text's colour does not tint a colour glyph
    Then the pixel at 59, 12 of the window is "#ff0000"
    And the pixel at 59, 27 of the window is "#0000ff"
    And the pixel at 99, 12 of the window is "#ff0000"
    And the pixel at 99, 27 of the window is "#0000ff"

  Scenario: A colour glyph is scaled to the text's size
    Then the pixel at 149, 13 of the window is "#ff0000"
    And the pixel at 149, 46 of the window is "#0000ff"
    And the pixel at 149, 52 of the window is "#000000"

  Scenario: The item's opacity fades a colour glyph
    Then the pixel at 19, 52 of the window is within 2 of "#800000"
    And the pixel at 19, 67 of the window is within 2 of "#000080"

  Scenario: A text style draws a colour glyph's picture, not its colour
    Then the pixel at 59, 52 of the window is "#ff0000"
    And the pixel at 59, 69 of the window is "#0000ff"
    And the pixel at 59, 70 of the window is "#0000ff"
