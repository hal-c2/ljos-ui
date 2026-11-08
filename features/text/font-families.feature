# Sources:
#   https://doc.qt.io/qt-6/qfont.html#setFamilies
#   https://doc.qt.io/qt-6/qfont.html#families
#   https://doc.qt.io/qt-6/qml-font.html
# Qt 6.12's QML font has no families member (qml6: 'Cannot assign to
# non-existent property "families"'); QFont has it, and an application sets
# it from C++. Checked against Qt 6.12.0's QFont on Linux: family() is the
# first of families(), setFamily replaces the whole list with the one
# family, setFamilies({}) leaves both empty, and two fonts whose lists differ
# are not equal. Ljós gives QML the member QFont has; runs on Ljós only.
# LjosSymbolsTest-Regular.ttf ("Ljos Symbols Test") has U+F8F0 at 600 units
# of 1000, LjosAlternateTest-Regular.ttf ("Ljos Alternate Test") the same
# character at 500, and with both loaded a text that names neither is set in
# Ljos Alternate Test, whose name comes first (font-fallback.feature). Noto
# Serif Test has no U+F8F0.

@ljos-only
Feature: A font's list of families
  A font names its families in order. A character the first family lacks
  is set in the next that has it, before any font the system falls back
  to. The first of the list is the font's family, and naming one family
  makes it the whole list.

  Scenario: A character the first family lacks is set in the next family that has it
    Given the font file "LjosSymbolsTest-Regular.ttf"
    And the font file "LjosAlternateTest-Regular.ttf"
    And the QML:
      """qml
      import QtQuick
      Item {
        FontLoader { source: "LjosSymbolsTest-Regular.ttf" }
        FontLoader { source: "LjosAlternateTest-Regular.ttf" }
        Text {
          objectName: "listed"; text: String.fromCodePoint(0xF8F0); font.pixelSize: 20
          font.families: ["Noto Serif Test", "Ljos Symbols Test"]
        }
        Text {
          objectName: "unlisted"; text: String.fromCodePoint(0xF8F0); font.pixelSize: 20
          font.family: "Noto Serif Test"
        }
      }
      """
    When 100 milliseconds pass
    Then the property "contentWidth" of "listed" is 11.984375
    And the property "contentWidth" of "unlisted" is 9.984375

  Scenario: The first family of the list is the font's family
    Given the QML:
      """qml
      import QtQuick
      Text {
        objectName: "t"
        property string first: font.family
        property string all: font.families.join("|")
        font.families: ["Liberation Mono", "Symbols Nerd Font Mono"]
      }
      """
    Then the property "first" of "t" is "Liberation Mono"
    And the property "all" of "t" is "Liberation Mono|Symbols Nerd Font Mono"

  Scenario: Naming one family makes it the whole list
    Given the QML:
      """qml
      import QtQuick
      Text {
        objectName: "t"
        font.families: ["Liberation Mono", "Symbols Nerd Font Mono"]
        property string all: font.families.join("|")
        function name() { font.family = "Liberation Serif" }
      }
      """
    When the method "name" of "t" is called
    Then the property "all" of "t" is "Liberation Serif"

  Scenario: A font that names only its family lists that family
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property font f: Qt.font({ family: "Liberation Serif" })
        property font listed: Qt.font({ families: ["Liberation Serif", "Liberation Mono"] })
        property string all: f.families.join("|")
        property string listedFamily: listed.family
        property bool same: f === Qt.font({ families: ["Liberation Serif"] })
        property bool differ: f === listed
      }
      """
    Then the property "all" of "root" is "Liberation Serif"
    And the property "listedFamily" of "root" is "Liberation Serif"
    And the property "same" of "root" is true
    And the property "differ" of "root" is false
