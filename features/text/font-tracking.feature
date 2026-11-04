# Sources:
#   https://doc.qt.io/qt-6/qfont.html#details
# Verified on Qt 6.11.2 (qml6, offscreen).
# LjosTrackTest-Regular.ttf (features/text/fonts, made by
# make-symbols-test.py there) has U+F8F0 at 600 units of 1000 and a tracking
# table ('trak', with the 'STAT' table that marks a font whose tracking is
# applied, as San Francisco has): 100 units more after each character at 12
# points and below, 100 fewer at 24 and above, interpolated between.
# Qt applies the tracking at the font's size in points, its pixel size times
# 72 over the logical dots per inch. On Linux (96) a 24-pixel text is 18
# points, where the tracking is 0, so two characters are 28.78125 wide. On
# macOS (72) a point is a pixel, and the same text is 23.96875 wide, as in
# Ljós, whose shaper takes the pixel size as the size in points everywhere.

Feature: A font's tracking
  A font with a tracking table is set with the space it asks for at the
  text's size: more between characters at small sizes, less at large ones,
  as macOS draws its system font.

  @qt-parity
  Scenario: Below the smallest size of the table, its first value is added
    Given the font file "LjosTrackTest-Regular.ttf"
    And the QML:
      """qml
      import QtQuick
      Item {
        FontLoader { source: "LjosTrackTest-Regular.ttf" }
        Text {
          objectName: "t"
          text: String.fromCodePoint(0xF8F0, 0xF8F0)
          font.family: "Ljos Track Test"
          font.pixelSize: 12
        }
      }
      """
    Then the property "contentWidth" of "t" is 16.78125

  # See above: Qt on Linux tracks this text as 18 points.
  @qt-diverges
  Scenario: At a large size the characters are set closer
    Given the font file "LjosTrackTest-Regular.ttf"
    And the QML:
      """qml
      import QtQuick
      Item {
        FontLoader { source: "LjosTrackTest-Regular.ttf" }
        Text {
          objectName: "t"
          text: String.fromCodePoint(0xF8F0, 0xF8F0)
          font.family: "Ljos Track Test"
          font.pixelSize: 24
        }
      }
      """
    Then the property "contentWidth" of "t" is 23.96875
