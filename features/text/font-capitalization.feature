# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#font.capitalization-prop
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#font.hintingPreference-prop
#   https://doc.qt.io/qt-6/qml-font.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: The capitalization and the hinting preference of a font
  font.capitalization changes the letters a text is drawn with and leaves
  the text itself as it was written. font.hintingPreference is a wish to
  the rasterizer and changes nothing a script can see but its own value.
  Numbers depend on the font file, so these scenarios compare items with
  each other.

  Rule: The text is drawn in the letters the font asks for

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property int how: Font.MixedCase
          property string words: "hello wORLD"
          Text { id: shown; objectName: "shown"; text: root.words; font.capitalization: root.how }
          Text { id: mixed; text: "hello wORLD" }
          Text { id: upper; text: "HELLO WORLD" }
          Text { id: lower; text: "hello world" }
          Text { id: first; text: "Hello WORLD" }
          property bool asWritten: shown.implicitWidth === mixed.implicitWidth
          property bool asUpper: shown.implicitWidth === upper.implicitWidth
          property bool asLower: shown.implicitWidth === lower.implicitWidth
          property bool asFirst: shown.implicitWidth === first.implicitWidth
          property bool differ: mixed.implicitWidth !== upper.implicitWidth
            && mixed.implicitWidth !== lower.implicitWidth && mixed.implicitWidth !== first.implicitWidth
        }
        """

    Scenario: The four texts it is compared with are of different widths
      Then the property "differ" of "root" is true
      And the property "asWritten" of "root" is true

    Scenario Outline: <how> draws the text <drawn>
      When the property "how" of "root" is set to <value>
      Then the property "<same>" of "root" is true
      And the property "text" of "shown" is "hello wORLD"

      Examples:
        | how               | value | same      | drawn                              |
        | Font.MixedCase    | 0     | asWritten | as it was written                  |
        | Font.AllUppercase | 1     | asUpper   | in capital letters                 |
        | Font.AllLowercase | 2     | asLower   | in small letters                   |
        | Font.Capitalize   | 4     | asFirst   | with a capital first in every word |

    Scenario: Going back to mixed case draws the text as written again
      When the property "how" of "root" is set to 1
      Then the property "asUpper" of "root" is true
      When the property "how" of "root" is set to 0
      Then the property "asWritten" of "root" is true

  Rule: Every letter stays one letter

    Scenario: A letter that has no single capital is left as it is
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          Text { id: shown; text: "straße"; font.capitalization: Font.AllUppercase }
          Text { id: kept; text: "STRAßE" }
          Text { id: two; text: "STRASSE" }
          property bool asKept: shown.implicitWidth === kept.implicitWidth
          property bool asTwo: shown.implicitWidth === two.implicitWidth
        }
        """
      Then the property "asKept" of "root" is true
      And the property "asTwo" of "root" is false

  Rule: Small capitals are capital letters of a smaller size

    # Small capitals are kept as a value and drawn as mixed case on Ljós.
    @backlog
    Scenario: Small capitals are of another width than the text as written, and as tall
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          Text { id: small; text: "hello World"; font.capitalization: Font.SmallCaps }
          Text { id: mixed; text: "hello World" }
          property bool other: small.implicitWidth !== mixed.implicitWidth
          property bool tall: small.implicitHeight === mixed.implicitHeight
        }
        """
      Then the property "other" of "root" is true
      And the property "tall" of "root" is true

  Rule: An editor shows its text in those letters too

    Scenario Outline: A <type> in capital letters is as wide as the capital text
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          <type> { id: shown; objectName: "shown"; text: "hello World"; font.capitalization: Font.AllUppercase }
          <type> { id: upper; text: "HELLO WORLD" }
          <type> { id: mixed; text: "hello World" }
          property bool asUpper: shown.implicitWidth === upper.implicitWidth
          property bool differ: mixed.implicitWidth !== upper.implicitWidth
        }
        """
      Then the property "asUpper" of "root" is true
      And the property "differ" of "root" is true
      And the property "text" of "shown" is "hello World"

      Examples:
        | type      |
        | TextInput |
        | TextEdit  |

  Rule: Both are members of the font value

    Scenario: A font says them, in Qt's numbers
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          Text { id: plain }
          Text { id: t; font.capitalization: Font.SmallCaps; font.hintingPreference: Font.PreferFullHinting }
          property string plainly: plain.font.capitalization + "," + plain.font.hintingPreference
          property string set: t.font.capitalization + "," + t.font.hintingPreference
          property string kind: typeof t.font.capitalization
          property string caps: [Font.MixedCase, Font.AllUppercase, Font.AllLowercase,
            Font.SmallCaps, Font.Capitalize].join(",")
          property string hints: [Font.PreferDefaultHinting, Font.PreferNoHinting,
            Font.PreferVerticalHinting, Font.PreferFullHinting].join(",")
        }
        """
      Then the property "plainly" of "root" is "0,0"
      And the property "set" of "root" is "3,3"
      And the property "kind" of "root" is "number"
      And the property "caps" of "root" is "0,1,2,3,4"
      And the property "hints" of "root" is "0,1,2,3"

    Scenario: A script lists them among a font's members, where Qt has them
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          Text { id: t }
          property string after
          function list() {
            var keys = []
            for (var k in t.font) keys.push(k)
            after = keys[keys.indexOf("capitalization") - 1] + ","
              + keys[keys.indexOf("hintingPreference") - 1]
          }
        }
        """
      When the method "list" of "root" is called
      Then the property "after" of "root" is "pixelSize,wordSpacing"

    Scenario: Qt.font takes them, and the capitalization is in the font's text
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property font made: Qt.font({ family: "Arial", pixelSize: 12,
            capitalization: Font.Capitalize, hintingPreference: Font.PreferNoHinting })
          property string members: made.capitalization + "," + made.hintingPreference
          property string text: String(made)
        }
        """
      Then the property "members" of "root" is "4,1"
      And the property "text" of "root" is "QFont(Arial,-1,12,5,400,0,0,0,0,0,4,0,0,0,0,1,,0,0)"

    Scenario: Changing one from a script changes the font, and two fonts that differ in it are not equal
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          Text { id: t; text: "hello World"; onFontChanged: root.changes++ }
          Text { id: lower; text: "hello world" }
          Text { id: other; font.hintingPreference: Font.PreferNoHinting }
          property int changes: 0
          property string seen
          property bool asLower: t.implicitWidth === lower.implicitWidth
          function change() {
            var n = changes
            t.font.capitalization = Font.AllLowercase
            seen = (changes - n) + "," + (t.font == lower.font) + "," + (other.font == lower.font)
          }
        }
        """
      Then the property "asLower" of "root" is false
      When the method "change" of "root" is called
      Then the property "seen" of "root" is "1,false,false"
      And the property "asLower" of "root" is true

  Rule: A control hands them on with the rest of its font

    Scenario: The controls in a control with capital letters have them too
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          objectName: "root"
          Pane {
            id: pane
            font.capitalization: Font.AllUppercase
            font.hintingPreference: Font.PreferNoHinting
            Column {
              Label { id: inner; text: "hello World" }
              Label { id: own; text: "hello World"; font.capitalization: Font.AllLowercase }
            }
          }
          Label { id: upper; text: "HELLO WORLD" }
          Label { id: lower; text: "hello world" }
          property string says: pane.font.capitalization + "," + inner.font.capitalization
            + "," + own.font.capitalization + "," + upper.font.capitalization
          property string hints: inner.font.hintingPreference + "," + own.font.hintingPreference
          property bool asUpper: inner.implicitWidth === upper.implicitWidth
          property bool asLower: own.implicitWidth === lower.implicitWidth
        }
        """
      Then the property "says" of "root" is "1,1,2,0"
      And the property "hints" of "root" is "1,1"
      And the property "asUpper" of "root" is true
      And the property "asLower" of "root" is true
