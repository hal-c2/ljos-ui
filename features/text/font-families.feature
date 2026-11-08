# Sources:
#   https://doc.qt.io/qt-6/qfont.html#setFamilies
#   https://doc.qt.io/qt-6/qfont.html#families
#   https://doc.qt.io/qt-6/qml-font.html
# Qt 6.12's QML font has no families member (qml6: 'Cannot assign to
# non-existent property "families"'); QFont has it, and an application sets
# it from C++. Checked against Qt 6.12.0's QFont on Linux: family() is the
# first of families(), setFamily replaces the whole list with the one
# family, setFamilies({}) leaves both empty, and two fonts whose lists differ
# are not equal. It keeps every name as given, an empty one or one with a
# line break in it among them (["", "Liberation Mono"] has the family ""),
# and a family the system lacks gives way to the next of the list before
# any face the system would put in its place: with ["No Such Family Ljos",
# "Liberation Mono"], QFontInfo's family is Liberation Mono and
# "Hamburgefonstiv" is 180 pixels wide at 20, as in Liberation Mono, where
# the missing family alone is set in Liberation Sans. Ljós gives QML the
# member QFont has; runs on Ljós only.
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

  Scenario: A family the system lacks gives way to the next family of the list
    Given the application uses the fonts installed on the system
    And the QML:
      """qml
      import QtQuick
      Text {
        objectName: "t"; text: "Hamburgefonstiv"; font.pixelSize: 20
        font.families: ["No Such Family Ljos", "Liberation Mono"]
      }
      """
    When 100 milliseconds pass
    Then the property "contentWidth" of "t" is 180

  Scenario: Every family of the list is kept as it is written
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property font f: Qt.font({ families: ["Liberation Serif", "", "Two\nLines"] })
        property font blankFirst: Qt.font({ families: ["", "Liberation Mono"] })
        property int count: f.families.length
        property bool blankKept: f.families[1] === ""
        property bool breakKept: f.families[2] === "Two\nLines"
        property bool differ: Qt.font({ families: ["A", "B\nC"] }) === Qt.font({ families: ["A", "B", "C"] })
        property string blankFamily: "[" + blankFirst.family + "]"
        property int blankCount: blankFirst.families.length
      }
      """
    Then the property "count" of "root" is 3
    And the property "blankKept" of "root" is true
    And the property "breakKept" of "root" is true
    And the property "differ" of "root" is false
    And the property "blankFamily" of "root" is "[]"
    And the property "blankCount" of "root" is 2

  Scenario: A font's list of families reads through an alias
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        Text { id: label; font.families: ["Liberation Mono", "Symbols Nerd Font Mono"] }
        property alias faces: label.font.families
        property string all: faces.join("|")
      }
      """
    Then the property "all" of "root" is "Liberation Mono|Symbols Nerd Font Mono"
