# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-fontinfo.html
#   https://doc.qt.io/qt-6/qfontinfo.html#fixedPitch
#   https://doc.qt.io/qt-6/qfontdatabase.html#systemFont
# Verified on Qt 6.12.0 (qml6, offscreen).
# FontInfo answers for the face a font resolves to, not the one it asks
# for. A face is fixed-pitch when every character it maps that moves the
# pen moves it as far: Ljos Alternate Test's one character is 500 units
# wide, Ljos Symbols Test has two widths, and Liberation Sans many. On
# Linux that is fontconfig's answer too (`fc-query`'s spacing is 100 for
# Ljos Alternate Test and Liberation Mono and none for the others), which
# is what Qt reports; Noto Sans Mono, whose characters are of three widths,
# is not fixed-pitch there either, and JetBrains Mono's Nerd Font, whose
# `post` table does not say so, is. Qt's QML has no systemFont: the family
# "monospace" resolves to the face the system names fixed-pitch
# (`fc-match monospace`), and FontInfo tells which it is.
# The weight and italics are those the font asks for, as in Qt, where a
# face without them is made bolder or slanted; bold is any weight above
# normal (450 is bold), and the style name is the face's own. The pixel
# size is the whole pixel the text is set at, and the point size is that
# many pixels in points (at 96 dots an inch): 10.5 points are 14 pixels,
# and 21 pixels are 15.75 points.

@qt-parity
Feature: What a font resolves to
  FontInfo tells the family, style and size of the face a font is set in,
  and whether every character of that face is as wide as the others, so
  an application can ask whether a family is fixed-pitch, and which
  family the system sets fixed-pitch text in.

  Scenario: A face whose characters are all one width is fixed-pitch
    Given the font file "LjosAlternateTest-Regular.ttf"
    And the font file "LjosSymbolsTest-Regular.ttf"
    And the QML:
      """qml
      import QtQuick
      Item {
        FontLoader { source: "LjosAlternateTest-Regular.ttf" }
        FontLoader { source: "LjosSymbolsTest-Regular.ttf" }
        FontInfo { objectName: "one"; font.family: "Ljos Alternate Test" }
        FontInfo { objectName: "two"; font.family: "Ljos Symbols Test" }
      }
      """
    When 100 milliseconds pass
    Then the property "fixedPitch" of "one" is true
    And the property "family" of "one" is "Ljos Alternate Test"
    And the property "fixedPitch" of "two" is false
    And the property "family" of "two" is "Ljos Symbols Test"

  Scenario: A font registered later is the face its family resolves to from then on
    Given the font file "LjosAlternateTest-Regular.ttf"
    And the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property bool missingAtFirst
        FontInfo { id: info; objectName: "info"; font.family: "Ljos Alternate Test" }
        FontLoader { id: loader }
        Component.onCompleted: {
          missingAtFirst = info.family !== "Ljos Alternate Test" && !info.fixedPitch
          loader.source = "LjosAlternateTest-Regular.ttf"
        }
      }
      """
    When 100 milliseconds pass
    Then the property "missingAtFirst" of "root" is true
    And the property "family" of "info" is "Ljos Alternate Test"
    And the property "fixedPitch" of "info" is true

  Scenario: A loader's handler of its status sees the face it loaded
    Given the font file "LjosAlternateTest-Regular.ttf"
    And the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property string familyAtFirst
        property string familyAtReady
        property bool fixedAtReady
        FontInfo { id: info; font.family: "Ljos Alternate Test" }
        FontLoader {
          id: loader
          onStatusChanged: if (status === FontLoader.Ready) {
            root.familyAtReady = info.family
            root.fixedAtReady = info.fixedPitch
          }
        }
        Component.onCompleted: {
          familyAtFirst = info.family
          loader.source = "LjosAlternateTest-Regular.ttf"
        }
      }
      """
    When 100 milliseconds pass
    Then the property "familyAtFirst" of "root" is not "Ljos Alternate Test"
    And the property "familyAtReady" of "root" is "Ljos Alternate Test"
    And the property "fixedAtReady" of "root" is true

  Scenario: A monospace family is fixed-pitch and a proportional one is not
    Given the application uses the fonts installed on the system
    And the QML:
      """qml
      import QtQuick
      Item {
        FontInfo { objectName: "mono"; font.family: "Liberation Mono" }
        FontInfo { objectName: "sans"; font.family: "Liberation Sans" }
      }
      """
    When 100 milliseconds pass
    Then the property "fixedPitch" of "mono" is true
    And the property "family" of "mono" is "Liberation Mono"
    And the property "fixedPitch" of "sans" is false
    And the property "family" of "sans" is "Liberation Sans"

  Scenario: The family monospace resolves to the system's fixed-pitch face
    Given the application uses the fonts installed on the system
    And the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        FontInfo { id: fixed; objectName: "fixed"; font.family: "monospace" }
        Text { id: generic; text: "Hamburgefonstiv"; font.family: "monospace" }
        Text { id: byName; text: "Hamburgefonstiv"; font.family: fixed.family }
        property bool named: fixed.family !== "" && fixed.family !== "monospace"
        property bool sameFace: byName.contentWidth === generic.contentWidth
      }
      """
    When 100 milliseconds pass
    Then the property "fixedPitch" of "fixed" is true
    And the property "named" of "root" is true
    And the property "sameFace" of "root" is true

  Scenario: The weight and italics are those the font asks for
    Given the font file "LjosAlternateTest-Regular.ttf"
    And the QML:
      """qml
      import QtQuick
      Item {
        FontLoader { source: "LjosAlternateTest-Regular.ttf" }
        FontInfo {
          objectName: "heavier"
          font { family: "Ljos Alternate Test"; weight: 450; italic: true }
        }
        FontInfo { objectName: "light"; font { family: "Ljos Alternate Test"; weight: 300 } }
      }
      """
    When 100 milliseconds pass
    Then the property "weight" of "heavier" is 450
    And the property "bold" of "heavier" is true
    And the property "italic" of "heavier" is true
    And the property "style" of "heavier" is 1
    And the property "styleName" of "heavier" is "Regular"
    And the property "weight" of "light" is 300
    And the property "bold" of "light" is false
    And the property "italic" of "light" is false
    And the property "style" of "light" is 0

  Scenario: The size is the whole pixel the text is set at
    Given the QML:
      """qml
      import QtQuick
      Item {
        FontInfo { objectName: "twelve"; font.pointSize: 12 }
        FontInfo { objectName: "points"; font.pointSize: 10.5 }
        FontInfo { objectName: "pixels"; font.pixelSize: 21 }
      }
      """
    When 100 milliseconds pass
    Then the property "pixelSize" of "twelve" is 16
    And the property "pointSize" of "twelve" is 12
    And the property "pixelSize" of "points" is 14
    And the property "pointSize" of "points" is 10.5
    And the property "pixelSize" of "pixels" is 21
    And the property "pointSize" of "pixels" is 15.75
