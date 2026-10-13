# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#copy-method
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#paste-method
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#textFormat-prop
#   https://doc.qt.io/qt-6/qml-qtquick-textinput.html#copy-method
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Copying and pasting formatted text
  A TextEdit copies HTML and the characters as plain text. A rich TextEdit
  pastes the HTML, from itself, another editor or another application, with
  its formats; a plain editor pastes the characters. A TextInput copies only
  the characters, and text that has no format of its own takes the format
  where it is pasted. The scenarios compare the body of a rich editor's
  HTML as the rich text scenarios do: double quotes read as single quotes
  and line breaks as bars.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 300; height: 200
        property string out
        function simple(html) {
          return html.replace(/^[\s\S]*<body[^>]*>\n?/, "").replace(/<\/body>[\s\S]*$/, "")
            .split(" style=\" margin-top:0px; margin-bottom:0px; margin-left:0px; margin-right:0px;"
                   + " -qt-block-indent:0; text-indent:0px;\"").join("").split("\"").join("'").split("\n").join("|")
        }
        function show(name) {
          const e = { target: target, bolded: bolded, plainEdit: plainEdit, field: field, md: md }[name]
          out = e.textFormat === TextEdit.RichText ? simple(e.text) : e.text.split("\n").join("|")
        }
        TextEdit { id: source; objectName: "source"; textFormat: TextEdit.RichText; text: "a<b>bold</b><i>it</i>c" }
        TextEdit { id: paragraphs; objectName: "paragraphs"; textFormat: TextEdit.RichText; text: "<p>one</p><p><b>two</b></p>" }
        TextEdit { id: target; objectName: "target"; textFormat: TextEdit.RichText; text: "xy" }
        TextEdit { id: bolded; objectName: "bolded"; textFormat: TextEdit.RichText; text: "a<b>bold</b>c" }
        TextEdit { id: plainEdit; objectName: "plainEdit"; textFormat: TextEdit.PlainText; text: "xy" }
        TextEdit { id: md; objectName: "md"; textFormat: TextEdit.MarkdownText; text: "ab" }
        TextInput { id: field; objectName: "field"; text: "xy" }
        TextInput { objectName: "plainField"; text: "PL" }
        TextEdit { objectName: "plainSource"; textFormat: TextEdit.PlainText; text: "PL" }
      }
      """

  Rule: Text copied from a rich TextEdit keeps its formats

    Background:
      When the method "select" of "source" is called with 2, 6
      And the method "copy" of "source" is called

    Scenario: A rich TextEdit pastes it with its formats
      When the property "cursorPosition" of "target" is set to 1
      And the method "paste" of "target" is called
      And the method "show" of "root" is called with "target"
      Then the property "out" of "root" is "<p>x<span style=' font-weight:700;'>old</span><span style=' font-style:italic;'>i</span>y</p>"

    Scenario Outline: A <editor> pastes its characters
      When the property "cursorPosition" of "<editor>" is set to 1
      And the method "paste" of "<editor>" is called
      And the method "show" of "root" is called with "<editor>"
      Then the property "out" of "root" is "xoldiy"

      Examples:
        | editor    |
        | plainEdit |
        | field     |

    Scenario: A Markdown TextEdit pastes it with its formats
      When the property "cursorPosition" of "md" is set to 1
      And the method "paste" of "md" is called
      And the method "show" of "root" is called with "md"
      Then the property "out" of "root" is "a**old***i*b||"

  Rule: Cut and paste moves formatted text

    Scenario: Bold text cut and pasted elsewhere stays bold
      When the method "select" of "bolded" is called with 1, 5
      And the method "cut" of "bolded" is called
      And the property "cursorPosition" of "bolded" is set to 0
      And the method "paste" of "bolded" is called
      And the method "show" of "root" is called with "bolded"
      Then the property "out" of "root" is "<p><span style=' font-weight:700;'>bold</span>ac</p>"

    Scenario: Undoing the paste takes the bold text away again
      When the method "select" of "bolded" is called with 1, 5
      And the method "cut" of "bolded" is called
      And the property "cursorPosition" of "bolded" is set to 0
      And the method "paste" of "bolded" is called
      And the method "undo" of "bolded" is called
      And the method "show" of "root" is called with "bolded"
      Then the property "out" of "root" is "<p>ac</p>"

  Rule: Paragraphs copied from a rich TextEdit paste as lines of plain text

    Scenario: A plain TextEdit pastes paragraphs as lines
      When the method "select" of "paragraphs" is called with 1, 6
      And the method "copy" of "paragraphs" is called
      And the property "cursorPosition" of "plainEdit" is set to 1
      And the method "paste" of "plainEdit" is called
      And the method "show" of "root" is called with "plainEdit"
      Then the property "out" of "root" is "xne|twy"

  Rule: Plain text pasted into rich text takes the format where it goes

    Background:
      When the method "selectAll" of "plainField" is called
      And the method "copy" of "plainField" is called

    Scenario: Text a TextInput copied is plain before a bold word
      When the property "cursorPosition" of "bolded" is set to 1
      And the method "paste" of "bolded" is called
      And the method "show" of "root" is called with "bolded"
      Then the property "out" of "root" is "<p>aPL<span style=' font-weight:700;'>bold</span>c</p>"

    Scenario: Text a TextInput copied is bold inside a bold word
      When the property "cursorPosition" of "bolded" is set to 3
      And the method "paste" of "bolded" is called
      And the method "show" of "root" is called with "bolded"
      Then the property "out" of "root" is "<p>a<span style=' font-weight:700;'>boPLld</span>c</p>"

    Scenario: Text a TextInput copied is bold after a bold word
      When the property "cursorPosition" of "bolded" is set to 5
      And the method "paste" of "bolded" is called
      And the method "show" of "root" is called with "bolded"
      Then the property "out" of "root" is "<p>a<span style=' font-weight:700;'>boldPL</span>c</p>"

    Scenario: Text a TextInput copied takes the format of the selection it replaces
      When the method "select" of "bolded" is called with 2, 4
      And the method "paste" of "bolded" is called
      And the method "show" of "root" is called with "bolded"
      Then the property "out" of "root" is "<p>a<span style=' font-weight:700;'>bPLd</span>c</p>"

  Rule: A plain TextEdit copies text that has no format

    Scenario: Text a plain TextEdit copied is not bold inside a bold word
      When the method "selectAll" of "plainSource" is called
      And the method "copy" of "plainSource" is called
      And the property "cursorPosition" of "bolded" is set to 3
      And the method "paste" of "bolded" is called
      And the method "show" of "root" is called with "bolded"
      Then the property "out" of "root" is "<p>a<span style=' font-weight:700;'>bo</span>PL<span style=' font-weight:700;'>ld</span>c</p>"

  Rule: HTML another application copied keeps its formats

    Scenario: A rich TextEdit pastes it inside a bold word with its own formats
      Given the HTML "<i>sl</i><span style='color:#ff0000'>red</span>" was copied in another application
      When the property "cursorPosition" of "bolded" is set to 3
      And the method "paste" of "bolded" is called
      And the method "show" of "root" is called with "bolded"
      Then the property "out" of "root" is "<p>a<span style=' font-weight:700;'>bo</span><span style=' font-style:italic;'>sl</span><span style=' color:#ff0000;'>red</span><span style=' font-weight:700;'>ld</span>c</p>"

    Scenario: A link keeps its address
      Given the HTML "<a href='http://e.org'>ln</a>" was copied in another application
      When the property "cursorPosition" of "target" is set to 1
      And the method "paste" of "target" is called
      And the method "show" of "root" is called with "target"
      Then the property "out" of "root" is "<p>x<a href='http://e.org'><span style=' text-decoration: underline; color:#0000ff;'>ln</span></a>y</p>"

    Scenario: A size keeps its points
      Given the HTML "<span style='font-size:20pt'>big</span>" was copied in another application
      When the property "cursorPosition" of "target" is set to 1
      And the method "paste" of "target" is called
      And the method "show" of "root" is called with "target"
      Then the property "out" of "root" is "<p>x<span style=' font-size:20pt;'>big</span>y</p>"

    Scenario: A plain TextEdit pastes its characters
      Given the HTML "<i>sl</i><span style='color:#ff0000'>red</span>" was copied in another application
      When the property "cursorPosition" of "plainEdit" is set to 1
      And the method "paste" of "plainEdit" is called
      And the method "show" of "root" is called with "plainEdit"
      Then the property "out" of "root" is "xslredy"

    Scenario: A TextInput pastes its characters
      Given the HTML "<i>sl</i><span style='color:#ff0000'>red</span>" was copied in another application
      When the property "cursorPosition" of "field" is set to 1
      And the method "paste" of "field" is called
      And the method "show" of "root" is called with "field"
      Then the property "out" of "root" is "xslredy"
