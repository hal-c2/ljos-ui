# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#color-prop
#   https://learn.microsoft.com/typography/opentype/spec/colr
#   https://learn.microsoft.com/typography/opentype/spec/cpal
# Verified on Qt 6.12.0 (qml6, offscreen).
# Windows' emoji font (Segoe UI Emoji) and newer Android's (Noto Color Emoji)
# draw a glyph as layers of outlines, each filled with a colour of the font's
# palette or with a gradient (COLR versions 0 and 1). A layer may take the
# text's own colour (palette index 0xFFFF).
# LjosColrTest-Regular.ttf (features/text/fonts, made by make-colr-test.py
# there) has two such glyphs, 20 by 20 pixels at 20 pixels per em, the top
# 16 pixels above the baseline, as the text's top: red above, blue below,
# and over the blue a square 4 pixels wide, 8 to 12 pixels across and 1 to 5
# above the baseline, in the text's colour. U+F8F2 is a version 0 glyph;
# U+F8F3 a version 1 glyph whose blue layer is a gradient from blue at the
# left edge to green at the right.
# Qt fills a version 1 glyph's text-coloured layer in the text's colour, but
# a version 0 glyph's in black whatever the text's colour (white, lime): the
# rasteriser's default foreground, not the text's. Ljós fills both in the
# text's colour, as the OpenType specification says.
# A text style draws its copy as the glyph's layers, not in the style's
# colour: Raised shows the gradient one row below the glyph.

@api-QtQuick.Text.color
Feature: Colour layer glyphs
  A font such as an emoji font may draw a glyph as layers of outlines in
  colours of its own. Such a glyph is drawn in those colours, scaled to the
  text's size; a layer in the text's colour takes the text's colour. The
  item's opacity fades it as it fades everything the item draws.

  Background:
    Given the font file "LjosColrTest-Regular.ttf"
    And the QML:
      """qml
      import QtQuick
      Rectangle {
        width: 240; height: 80; color: "black"
        FontLoader { id: emoji; source: "LjosColrTest-Regular.ttf" }
        Text { x: 10; y: 10; text: ""; font.family: emoji.name; font.pixelSize: 20; color: "white" }
        Text { x: 50; y: 10; text: ""; font.family: emoji.name; font.pixelSize: 20; color: "white" }
        Text { x: 90; y: 10; text: ""; font.family: emoji.name; font.pixelSize: 20; color: "lime" }
        Text { x: 130; y: 10; text: ""; font.family: emoji.name; font.pixelSize: 20; color: "transparent" }
        Text { x: 170; y: 10; text: ""; font.family: emoji.name; font.pixelSize: 40; color: "white" }
        Text { x: 10; y: 50; text: ""; font.family: emoji.name; font.pixelSize: 20; color: "white"; opacity: 0.5 }
        Text { x: 50; y: 50; text: ""; font.family: emoji.name; font.pixelSize: 20; color: "white"; style: Text.Raised; styleColor: "yellow" }
        Text { x: 130; y: 50; text: ""; font.family: emoji.name; font.pixelSize: 20; color: "lime" }
      }
      """
    When 100 milliseconds pass

  @qt-parity
  Scenario: A colour layer glyph is drawn in its palette's colours
    Then the pixel at 19, 12 of the window is "#ff0000"
    And the pixel at 14, 27 of the window is "#0000ff"
    And the pixel at 19, 9 of the window is "#000000"
    And the pixel at 9, 20 of the window is "#000000"

  @qt-parity
  Scenario: A version 1 glyph's gradient layer is drawn as its gradient
    Then the pixel at 50, 27 of the window is within 8 of "#0006f9"
    And the pixel at 69, 27 of the window is within 8 of "#00f906"
    And the pixel at 59, 12 of the window is "#ff0000"

  @qt-parity
  Scenario: A layer in the text's colour takes the text's colour
    Then the pixel at 60, 23 of the window is "#ffffff"
    And the pixel at 140, 63 of the window is "#00ff00"

  # Qt fills a version 0 glyph's text-coloured layer in black, whatever the
  # text's colour; the specification says the text's colour.
  @qt-diverges
  Scenario: A version 0 glyph's layer in the text's colour takes the text's colour
    Then the pixel at 20, 23 of the window is "#ffffff"
    And the pixel at 100, 23 of the window is "#00ff00"

  @qt-parity
  Scenario: A transparent text still shows its colour glyph's other layers
    Then the pixel at 140, 12 of the window is "#ff0000"
    And the pixel at 130, 23 of the window is within 8 of "#0006f9"
    And the pixel at 140, 23 of the window is within 8 of "#008679"

  @qt-parity
  Scenario: A colour layer glyph is scaled to the text's size
    Then the pixel at 180, 15 of the window is "#ff0000"
    And the pixel at 180, 45 of the window is "#0000ff"
    And the pixel at 180, 52 of the window is "#000000"

  @qt-parity
  Scenario: The item's opacity fades a colour layer glyph
    Then the pixel at 19, 52 of the window is within 2 of "#800000"
    And the pixel at 14, 67 of the window is within 2 of "#000080"

  @qt-parity
  Scenario: A text style draws a colour layer glyph's layers, not its colour
    Then the pixel at 59, 52 of the window is "#ff0000"
    And the pixel at 59, 70 of the window is within 8 of "#007986"
    And the pixel at 59, 71 of the window is "#000000"
