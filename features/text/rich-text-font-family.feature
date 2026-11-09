# Sources:
#   https://doc.qt.io/qt-6/richtext-html-subset.html#css-properties
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#cursorSelection-prop
#   https://www.w3.org/TR/css-fonts-4/#font-family-prop
# Verified on Qt 6.12.0 (qml6, offscreen, Linux).
# font-family is a list. A quoted name is one family whatever it holds,
# commas and semicolons too; unquoted words are one family, each run of
# blanks between them one space. The first family is the one the text
# names (cursorSelection.font.family reads it); the rest are tried after it
# for the face. Measured with "Hamburgefonstiv" at 20 pixels:
# 'No Such Family Ljos, Liberation Mono' is 179.765625 wide, the face
# fontconfig puts in place of the whole missing name, while the unquoted
# No Such Family Ljos, Liberation Mono is 180, Liberation Mono's. An empty
# family first or between two others leaves the declaration out; one after
# a trailing comma does not. Qt writes each family quoted, in single quotes
# unless the name holds one, and joined by commas without a space:
# font-family:'No Such Family Ljos','Liberation Mono' and
# font-family:&quot;O'Brien, X&quot;.

@qt-parity
Feature: The families rich text names
  A style's font-family names the families of its text in order. A name in
  quotes is one family, commas and all, so a family with a comma in its
  name is kept when an editor writes its text and reads it back.

  Background:
    Given the application uses the fonts installed on the system

  Rule: A quoted name is one family; a comma outside quotes starts the next

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"; width: 600; height: 400
          property string read
          function span(family) { return "<span style=\"font-family:" + family + "\">Hamburgefonstiv</span>" }
          function family(css) { edit.text = span(css); edit.cursorPosition = 3; read = edit.cursorSelection.font.family }
          function bold(css) { edit.text = "<span style=\"" + css + "\">Hamburgefonstiv</span>"; edit.cursorPosition = 3; read = edit.cursorSelection.font.bold }
          component R: Text { textFormat: Text.RichText; font.family: "Liberation Sans"; font.pixelSize: 20 }
          R { id: quoted; text: root.span("'No Such Family Ljos, Liberation Mono'") }
          R { id: listed; text: root.span("No Such Family Ljos, Liberation Mono") }
          R { id: missing; text: root.span("'No Such Family Ljos Liberation Mono'") }
          R { id: mono; text: root.span("'Liberation Mono'") }
          R { id: sans; text: root.span("'Liberation Sans'") }
          property bool quotedMissing: quoted.contentWidth === missing.contentWidth && quoted.height === missing.height
          property bool quotedMono: quoted.contentWidth === mono.contentWidth
          property bool listedMono: listed.contentWidth === mono.contentWidth && listed.height === mono.height
          property bool missingMono: missing.contentWidth === mono.contentWidth
          property bool sansMono: sans.contentWidth === mono.contentWidth
          TextEdit { id: edit; y: 200; width: 600; textFormat: TextEdit.RichText; font.family: "Liberation Sans"; font.pixelSize: 20 }
        }
        """
      When 100 milliseconds pass

    Scenario: A family in quotes with a comma in it is set as one family the system lacks
      Then the property "quotedMissing" of "root" is true
      And the property "quotedMono" of "root" is false

    Scenario: Families a comma parts are tried in order
      Then the property "listedMono" of "root" is true
      And the property "missingMono" of "root" is false
      And the property "sansMono" of "root" is false

    Scenario Outline: font-family:<css> names the family "<family>"
      When the method "family" of "root" is called with "<css>"
      Then the property "read" of "root" is "<family>"

      Examples:
        | css                                     | family                               |
        | 'No Such Family Ljos, Liberation Mono'  | No Such Family Ljos, Liberation Mono |
        | No Such Family Ljos, Liberation Mono    | No Such Family Ljos                  |
        | 'Liberation Mono', 'Liberation Serif'   | Liberation Mono                      |
        | 'A, B' , 'C'                            | A, B                                 |
        | 'A; B'                                  | A; B                                 |
        | &quot;A, B&quot;                          | A, B                                 |
        | 'O\\\\'Brien, X'                        | O'Brien, X                           |
        | A\\\\, B                                | A, B                                 |
        | 'A' B, C                                | A B                                  |
        | A 'B'                                   | A B                                  |
        | \\u0020 Liberation   Mono \\u0020       | Liberation Mono                      |
        | 3D Font                                 | 3D Font                              |
        | 'A',                                    | A                                    |
        | 'A' !important                          | A                                    |
        | , Liberation Mono                       | Liberation Sans                      |
        | A,,B                                    | Liberation Sans                      |
        | '', Liberation Mono                     | Liberation Sans                      |
        | 'unterminated                           | Liberation Sans                      |

    Scenario: A semicolon in a quoted family does not end the declaration
      When the method "bold" of "root" is called with "font-family:'A; B'; font-weight:600"
      Then the property "read" of "root" is "true"

  Rule: A family an editor writes reads back as it was

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"; width: 600; height: 400
          property string read; property string written
          property bool sameWidth: shown.contentWidth === mono.contentWidth
          function name(family) {
            edit.text = "<p>Hamburgefonstiv</p>"; edit.selectAll()
            let f = edit.cursorSelection.font; f.family = family; edit.cursorSelection.font = f
            copy()
          }
          function list(css) { edit.text = "<p><span style=\"font-family:" + css + "\">Hamburgefonstiv</span></p>"; copy() }
          function copy() {
            written = edit.text.match(/<span style="([^"]*)"/)[1].trim()
            other.text = edit.text
            other.cursorPosition = 3
            read = other.cursorSelection.font.family
            shown.text = edit.text
          }
          TextEdit { id: edit; width: 600; textFormat: TextEdit.RichText; font.family: "Liberation Sans"; font.pixelSize: 20 }
          TextEdit { id: other; y: 100; width: 600; textFormat: TextEdit.RichText; font.family: "Liberation Sans"; font.pixelSize: 20 }
          Text { id: shown; y: 200; textFormat: Text.RichText; font.family: "Liberation Sans"; font.pixelSize: 20 }
          Text { id: mono; y: 300; text: "Hamburgefonstiv"; font.family: "Liberation Mono"; font.pixelSize: 20 }
        }
        """

    Scenario Outline: The family "<family>" is written as <written> and reads back whole
      When the method "name" of "root" is called with "<family>"
      Then the property "written" of "root" is "<written>"
      And the property "read" of "root" is "<family>"

      Examples:
        | family                               | written                                              |
        | No Such Family Ljos, Liberation Mono | font-family:'No Such Family Ljos, Liberation Mono';  |
        | O'Brien, X                           | font-family:&quot;O'Brien, X&quot;;                  |
        | A; B                                 | font-family:'A; B';                                  |
        | A&B<C                                | font-family:'A&amp;B&lt;C';                          |

    Scenario: Families a comma parts are written and read back in order
      When the method "list" of "root" is called with "No Such Family Ljos, Liberation Mono"
      And 100 milliseconds pass
      Then the property "written" of "root" is "font-family:'No Such Family Ljos','Liberation Mono';"
      And the property "read" of "root" is "No Such Family Ljos"
      And the property "sameWidth" of "root" is true
