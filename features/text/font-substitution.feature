# Sources:
#   https://doc.qt.io/qt-6/qml-font.html
#   https://doc.qt.io/qt-6/qfont.html#details
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#baselineOffset-prop
# Verified on Qt 6.11.2 (qml6, offscreen).
# Which family the system puts in a missing one's place is the machine's own:
# fontconfig names Nimbus Sans for Helvetica where URW's fonts are installed
# (`fc-match Helvetica`), and there Helvetica bold at 16 pixels is 23 pixels
# tall with its baseline at 17.109375, against 18 and 14.484375 for Liberation
# Sans. The scenarios compare the two families rather than state numbers.
# A generic family is the system's too: `fc-match monospace` names the
# machine's fixed-pitch face, and Qt sets "monospace" text in it.

Feature: A font family that is not installed
  A family the machine does not have is set in the family the system's font
  matching puts in its place, with that family's ascent, descent and widths;
  with only the font Ljós carries, it is set in that font. A generic family,
  serif or monospace, is the face the system's matching names for it.

  @qt-parity
  Scenario: A missing family takes the measurements of the family the system puts in its place
    Given the application uses the fonts installed on the system
    And the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property bool sameFace: named.height === stand.height
          && named.baselineOffset === stand.baselineOffset
          && named.contentWidth === stand.contentWidth
        Text { id: named; text: "Tokyo"; font.family: "Helvetica"; font.bold: true; font.pixelSize: 16 }
        Text { id: stand; text: "Tokyo"; font.family: "Nimbus Sans"; font.bold: true; font.pixelSize: 16 }
      }
      """
    When 100 milliseconds pass
    Then the property "sameFace" of "root" is true

  @qt-parity
  Scenario: A generic family is set in the face the system names for it
    Given the application uses the fonts installed on the system
    And the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property bool fixedPitch: thin.contentWidth === wide.contentWidth
        property bool serifIsItsOwn: serif.contentWidth !== sans.contentWidth
        property bool capitalised: thin.contentWidth === upper.contentWidth && thin.height === upper.height
        Text { id: thin; text: "iiii"; font.family: "monospace"; font.pixelSize: 14 }
        Text { id: wide; text: "MMMM"; font.family: "monospace"; font.pixelSize: 14 }
        Text { id: upper; text: "iiii"; font.family: "Monospace"; font.pixelSize: 14 }
        Text { id: serif; text: "Hamburgefonstiv"; font.family: "serif"; font.pixelSize: 14 }
        Text { id: sans; text: "Hamburgefonstiv"; font.family: "sans-serif"; font.pixelSize: 14 }
      }
      """
    When 100 milliseconds pass
    Then the property "fixedPitch" of "root" is true
    And the property "serifIsItsOwn" of "root" is true
    And the property "capitalised" of "root" is true

  # The oracle reads the machine's fonts and cannot be limited to one.
  @ljos-only
  Scenario: With only the font Ljós carries, a missing family is set in it
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        Text { objectName: "named"; text: "Tokyo"; font.family: "Helvetica"; font.bold: true; font.pixelSize: 16 }
      }
      """
    When 100 milliseconds pass
    Then the property "height" of "named" is 18
    And the property "baselineOffset" of "named" is 14.484375
