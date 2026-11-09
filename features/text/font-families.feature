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
# the missing family alone is set in Liberation Sans. Each name of the list
# is one family whatever it holds: with ["No Such Family Ljos, Liberation
# Mono", "Liberation Serif"] or ["\"Liberation Mono\"", "Liberation Serif"],
# QFontInfo's family is Liberation Serif and the text is as wide as in
# Liberation Serif. So is a list of one: with ["No Such Family Ljos,
# Liberation Mono"] or ["\"Liberation Mono\""] alone, QFontInfo's family is
# not Liberation Mono and the text is 179.766 pixels wide, as in the
# desktop's default font, not 180. One empty name is a list too:
# setFamilies({""}) lists "" and equals setFamily(""), and is not
# setFamilies({}), which lists none. A list of one plain name equals
# setFamily of it. Where Qt reads a lone family as one name too, a
# font.family in Ljós is read as a style sheet writes one, commas and quotes
# and all, so a list of one name with a comma or quotes in it is not the
# family of that text there.
# Ljós gives QML the member QFont has; runs on Ljós only.
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
      Item {
        objectName: "root"
        Text {
          objectName: "t"; text: "Hamburgefonstiv"; font.pixelSize: 20
          font.families: ["No Such Family Ljos", "Liberation Mono"]
        }
        Text {
          objectName: "made"; text: "Hamburgefonstiv"
          font: Qt.font({ families: ["No Such Family Ljos", "Liberation Mono"], pixelSize: 20 })
        }
      }
      """
    When 100 milliseconds pass
    Then the property "contentWidth" of "t" is 180
    And the property "contentWidth" of "made" is 180

  Scenario: A name of the list is one family, whatever characters it holds
    Given the application uses the fonts installed on the system
    And the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        Text { id: serif; text: "Hamburgefonstiv"; font.pixelSize: 20; font.family: "Liberation Serif" }
        Text {
          id: comma; text: "Hamburgefonstiv"; font.pixelSize: 20
          font.families: ["No Such Family Ljos, Liberation Mono", "Liberation Serif"]
        }
        Text {
          id: quoted; text: "Hamburgefonstiv"; font.pixelSize: 20
          font.families: ["\"Liberation Mono\"", "Liberation Serif"]
        }
        Text {
          id: madeComma; text: "Hamburgefonstiv"
          font: Qt.font({ families: ["No Such Family Ljos, Liberation Mono", "Liberation Serif"], pixelSize: 20 })
        }
        Text {
          id: madeQuoted; text: "Hamburgefonstiv"
          font: Qt.font({ families: ["\"Liberation Mono\"", "Liberation Serif"], pixelSize: 20 })
        }
        property bool commaOne: comma.contentWidth === serif.contentWidth
        property bool quotedOne: quoted.contentWidth === serif.contentWidth
        property bool madeCommaOne: madeComma.contentWidth === serif.contentWidth
        property bool madeQuotedOne: madeQuoted.contentWidth === serif.contentWidth
      }
      """
    When 100 milliseconds pass
    Then the property "commaOne" of "root" is true
    And the property "quotedOne" of "root" is true
    And the property "madeCommaOne" of "root" is true
    And the property "madeQuotedOne" of "root" is true

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

  Scenario: One empty name is a list of one, not a list of none
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property font none: Qt.font({ families: [] })
        property font blank: Qt.font({ families: [""] })
        Text { id: label; font.families: [""] }
        property int noneCount: none.families.length
        property int blankCount: blank.families.length
        property int boundCount: label.font.families.length
        property bool same: none === blank
      }
      """
    Then the property "noneCount" of "root" is 0
    And the property "blankCount" of "root" is 1
    And the property "boundCount" of "root" is 1
    And the property "same" of "root" is false

  Scenario: A list a script holds names the families, bound or through an alias
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        component Labelled: Item {
          property alias faces: inner.font.families
          property alias label: inner
          Text { id: inner }
        }
        property var names: ["Liberation Mono", "Symbols Nerd Font Mono"]
        property var others: ["Liberation Serif", "Liberation Sans"]
        Text { id: bound; font.families: root.names }
        Labelled { id: aliased; faces: root.names }
        Labelled { id: written }
        property string boundAll: bound.font.families.join("|")
        property string aliasedAll: aliased.label.font.families.join("|")
        property string writtenAll: written.label.font.families.join("|")
        function rename() { written.faces = root.others }
      }
      """
    When the method "rename" of "root" is called
    Then the property "boundAll" of "root" is "Liberation Mono|Symbols Nerd Font Mono"
    And the property "aliasedAll" of "root" is "Liberation Mono|Symbols Nerd Font Mono"
    And the property "writtenAll" of "root" is "Liberation Serif|Liberation Sans"

  Scenario: One empty family is a list of one empty name, however it is named
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        Text { id: named; font.family: "" }
        Text { id: listed; font.families: [""] }
        Text { id: plain; font.family: "Liberation Serif" }
        Text { id: plainListed; font.families: ["Liberation Serif"] }
        property bool bound: named.font === listed.font
        property int boundCount: named.font.families.length
        property bool plainBound: plain.font === plainListed.font
        property bool made: Qt.font({ family: "" }) === Qt.font({ families: [""] })
        property int madeCount: Qt.font({ family: "" }).families.length
        property bool madeNone: Qt.font({ family: "" }) === Qt.font({ families: [] })
      }
      """
    Then the property "bound" of "root" is true
    And the property "boundCount" of "root" is 1
    And the property "plainBound" of "root" is true
    And the property "made" of "root" is true
    And the property "madeCount" of "root" is 1
    And the property "madeNone" of "root" is false

  Scenario: A list of one name is that one family, whatever characters it holds
    Given the application uses the fonts installed on the system
    And the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        Text { id: mono; text: "Hamburgefonstiv"; font.pixelSize: 20; font.family: "Liberation Mono" }
        Text { id: comma; text: "Hamburgefonstiv"; font.pixelSize: 20
               font.families: ["No Such Family Ljos, Liberation Mono"] }
        Text { id: quoted; text: "Hamburgefonstiv"; font.pixelSize: 20
               font.families: ["\"Liberation Mono\""] }
        Text {
          id: madeComma; text: "Hamburgefonstiv"
          font: Qt.font({ families: ["No Such Family Ljos, Liberation Mono"], pixelSize: 20 })
        }
        Text {
          id: madeQuoted; text: "Hamburgefonstiv"
          font: Qt.font({ families: ["\"Liberation Mono\""], pixelSize: 20 })
        }
        property bool commaMono: comma.contentWidth === mono.contentWidth
        property bool quotedMono: quoted.contentWidth === mono.contentWidth
        property bool madeCommaMono: madeComma.contentWidth === mono.contentWidth
        property bool madeQuotedMono: madeQuoted.contentWidth === mono.contentWidth
        property string quotedName: quoted.font.families.join("|")
        property string madeQuotedName: madeQuoted.font.families.join("|")
      }
      """
    When 100 milliseconds pass
    Then the property "commaMono" of "root" is false
    And the property "quotedMono" of "root" is false
    And the property "madeCommaMono" of "root" is false
    And the property "madeQuotedMono" of "root" is false
    And the property "quotedName" of "root" is "\"Liberation Mono\""
    And the property "madeQuotedName" of "root" is "\"Liberation Mono\""
