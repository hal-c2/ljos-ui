# Sources:
#   https://doc.qt.io/qt-6/qfont.html#details
#   https://doc.qt.io/qt-6/qfontdatabase.html#details
#   https://doc.qt.io/qt-6/qml-qtquick-fontloader.html
# Checked against Qt 6.11.2 (qml6, offscreen) on Linux: there a private-use
# character is set in an installed font that has it (fontconfig's list for
# the text), but a font the application loaded is not in that list, and the
# characters below are drawn as boxes, 13.125 pixels wide.
# LjosSymbolsTest-Regular.ttf (features/text/fonts, made by
# make-symbols-test.py there) has two private-use characters no installed
# font has, U+F8F0 at 600 units of 1000 and U+10FFF0 at 700, so a text's
# width says whether it was set in that font or drawn as a box. Private-use
# characters belong to no script: a fallback chosen by script finds nothing
# for them.

Feature: A character no font of the style has
  When neither a text's families nor the fonts its script falls back to
  have a character, the character is set in a font that has it, among all
  the fonts the application knows: those it loaded and those of the system.
  It is drawn as a box only when no font has it.

  # Qt on Linux leaves out the fonts an application loaded (see above);
  # Ljós looks through every font it knows, on every platform.
  @qt-diverges
  Scenario: A private-use character is set in a loaded font that has it
    Given the font file "LjosSymbolsTest-Regular.ttf"
    And the QML:
      """qml
      import QtQuick
      Item {
        FontLoader { source: "LjosSymbolsTest-Regular.ttf" }
        Text { objectName: "bmp"; text: String.fromCodePoint(0xF8F0); font.pixelSize: 20 }
        Text { objectName: "astral"; text: String.fromCodePoint(0x10FFF0); font.pixelSize: 20 }
      }
      """
    When 100 milliseconds pass
    Then the property "contentWidth" of "bmp" is 11.984375
    And the property "contentWidth" of "astral" is 13.984375

  # As above.
  @qt-diverges
  Scenario: Only the character no family has is set in the font that has it
    Given the font file "LjosSymbolsTest-Regular.ttf"
    And the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property real symbol: both.contentWidth - word.contentWidth
        FontLoader { source: "LjosSymbolsTest-Regular.ttf" }
        Text { id: word; text: "main"; font.pixelSize: 20 }
        Text { id: both; text: "main" + String.fromCodePoint(0xF8F0); font.pixelSize: 20 }
      }
      """
    When 100 milliseconds pass
    Then the property "symbol" of "root" is 11.984375
