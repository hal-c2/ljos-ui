# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#textFormat-prop
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#getFormattedText-method
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#cursorSelection-prop
#   https://doc.qt.io/qt-6/qml-qtquick-textselection.html
# Verified on Qt 6.11.2 (qml6, offscreen; key input over its vnc platform).

@qt-parity
Feature: Editing rich text in a TextEdit
  A TextEdit whose textFormat is RichText keeps the formatting of its text
  while the user and programs edit it, and its text reads back as HTML. The
  scenarios compare the body of that HTML: what Qt writes before it (a
  document type, a style sheet, the default font) is left out, and so is the
  style of a paragraph that has no margins of its own. Double quotes read as
  single quotes and line breaks as bars, so that the HTML fits in a step.

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
        function html() { out = simple(edit.text) }
        function plain() { out = edit.getText(0, edit.length).split("\u2029").join("|") }
        function selected() { out = edit.selectedText.split("\u2029").join("|") }
        function fragment() {
          const h = edit.getFormattedText(2, 4)
          out = h.substring(h.indexOf("<!--StartFragment-->") + 20, h.indexOf("<!--EndFragment-->")).split("\"").join("'")
        }
        function format() { const s = edit.cursorSelection; out = [s.font.bold, s.font.italic, s.color, s.alignment, s.text].join(",") }
        function bolden() { let f = edit.cursorSelection.font; f.bold = true; edit.cursorSelection.font = f }
        function held() { const f = edit.cursorSelection.font; f.bold = true }
        function redden() { edit.cursorSelection.color = "red" }
        function centre() { edit.cursorSelection.alignment = Qt.AlignHCenter }
        function retype() { edit.cursorSelection.text = "Z" }
        TextEdit { id: edit; objectName: "edit"; width: 280; focus: true; textFormat: TextEdit.RichText; text: "a<b>bold</b>c" }
      }
      """

  Rule: The text reads back as HTML

    Scenario: The text a program gave reads back as a paragraph of HTML
      When the method "html" of "root" is called
      Then the property "out" of "root" is "<p>a<span style=' font-weight:700;'>bold</span>c</p>"

    Scenario: The length counts characters, not markup
      Then the property "length" of "edit" is 6

    Scenario: getText gives the characters, paragraphs apart
      When the property "cursorPosition" of "edit" is set to 3
      And the user presses "Return"
      And the method "plain" of "root" is called
      Then the property "out" of "root" is "abo|ldc"

    Scenario: getFormattedText gives a fragment of HTML
      When the method "fragment" of "root" is called
      Then the property "out" of "root" is "<span style=' font-weight:700;'>ol</span>"

  Rule: Typed text takes the format of the text before it

    Scenario: Typing inside a bold word types bold text
      When the property "cursorPosition" of "edit" is set to 3
      And the user types "x"
      And the method "html" of "root" is called
      Then the property "out" of "root" is "<p>a<span style=' font-weight:700;'>boxld</span>c</p>"

    Scenario: Typing after a bold word goes on in bold
      When the property "cursorPosition" of "edit" is set to 5
      And the user types "x"
      And the method "html" of "root" is called
      Then the property "out" of "root" is "<p>a<span style=' font-weight:700;'>boldx</span>c</p>"

    Scenario: Typing before a bold word types plain text
      When the property "cursorPosition" of "edit" is set to 1
      And the user types "x"
      And the method "html" of "root" is called
      Then the property "out" of "root" is "<p>ax<span style=' font-weight:700;'>bold</span>c</p>"

    Scenario: Typing over a selection takes the format of what it replaces
      When the method "select" of "edit" is called with 1, 5
      And the user types "x"
      And the method "html" of "root" is called
      Then the property "out" of "root" is "<p>a<span style=' font-weight:700;'>x</span>c</p>"

    Scenario: Undoing the typing gives the text back as it was
      When the property "cursorPosition" of "edit" is set to 3
      And the user types "xy"
      And the method "undo" of "edit" is called
      And the method "html" of "root" is called
      Then the property "out" of "root" is "<p>a<span style=' font-weight:700;'>bold</span>c</p>"

  Rule: Paragraphs split and join with their formats

    Scenario: Return splits a paragraph and both halves keep their formats
      When the property "cursorPosition" of "edit" is set to 3
      And the user presses "Return"
      And the method "html" of "root" is called
      Then the property "out" of "root" is "<p>a<span style=' font-weight:700;'>bo</span></p>|<p><span style=' font-weight:700;'>ld</span>c</p>"
      And the property "lineCount" of "edit" is 2

    Scenario: Backspace at the start of a paragraph joins it to the one before
      When the property "cursorPosition" of "edit" is set to 3
      And the user presses "Return"
      And the user presses "Backspace"
      And the method "html" of "root" is called
      Then the property "out" of "root" is "<p>a<span style=' font-weight:700;'>bold</span>c</p>"

    Scenario: A selection across paragraphs reads with a paragraph separator
      When the property "cursorPosition" of "edit" is set to 3
      And the user presses "Return"
      And the method "select" of "edit" is called with 2, 5
      And the method "selected" of "root" is called
      Then the property "out" of "root" is "o|l"

  Rule: Deleting and undoing keep the formatting

    Scenario: Deleted bold text comes back bold when the deletion is undone
      When the method "select" of "edit" is called with 1, 5
      And the user presses "Delete"
      And the method "undo" of "edit" is called
      And the method "html" of "root" is called
      Then the property "out" of "root" is "<p>a<span style=' font-weight:700;'>bold</span>c</p>"

    Scenario: Redoing the deletion takes the bold text away again
      When the method "select" of "edit" is called with 1, 5
      And the user presses "Delete"
      And the method "undo" of "edit" is called
      And the method "redo" of "edit" is called
      And the method "html" of "root" is called
      Then the property "out" of "root" is "<p>ac</p>"

    Scenario: remove takes out part of a bold word
      When the method "remove" of "edit" is called with 2, 4
      And the method "html" of "root" is called
      Then the property "out" of "root" is "<p>a<span style=' font-weight:700;'>bd</span>c</p>"

  Rule: A program inserts HTML

    Scenario: insert puts formatted text inside a bold word
      When the method "insert" of "edit" is called with 3, "<i>X</i>"
      And the method "html" of "root" is called
      Then the property "out" of "root" is "<p>a<span style=' font-weight:700;'>bo</span><span style=' font-style:italic;'>X</span><span style=' font-weight:700;'>ld</span>c</p>"

    Scenario: Text a program inserts has only the format its HTML gives it
      When the method "insert" of "edit" is called with 3, "y"
      And the method "html" of "root" is called
      Then the property "out" of "root" is "<p>a<span style=' font-weight:700;'>bo</span>y<span style=' font-weight:700;'>ld</span>c</p>"

    Scenario: append adds a paragraph of HTML
      When the method "append" of "edit" is called with "<i>next</i>"
      And the method "html" of "root" is called
      Then the property "out" of "root" is "<p>a<span style=' font-weight:700;'>bold</span>c</p>|<p><span style=' font-style:italic;'>next</span></p>"

  Rule: cursorSelection reads and changes the format of the selection

    Scenario: The cursor's format is that of the character before it
      When the property "cursorPosition" of "edit" is set to 3
      And the method "format" of "root" is called
      Then the property "out" of "root" is "true,false,#000000,1,"

    Scenario: cursorSelection has the selected text
      When the method "select" of "edit" is called with 1, 5
      And the method "format" of "root" is called
      Then the property "out" of "root" is "true,false,#000000,1,bold"

    # The font assigned is a whole font, and so a format of its own beside
    # the bold the text had: the two are written as two spans.
    Scenario: A toolbar bolds the selection
      When the method "select" of "edit" is called with 5, 6
      And the method "bolden" of "root" is called
      And the method "html" of "root" is called
      Then the property "out" of "root" is "<p>a<span style=' font-weight:700;'>bold</span><span style=' font-weight:700;'>c</span></p>"

    # Writing a member of the font held in a constant writes the font back
    # to the selection, as assigning it would.
    Scenario: A toolbar bolds the selection through the font it holds
      When the method "select" of "edit" is called with 5, 6
      And the method "held" of "root" is called
      And the method "html" of "root" is called
      Then the property "out" of "root" is "<p>a<span style=' font-weight:700;'>bold</span><span style=' font-weight:700;'>c</span></p>"

    Scenario: Bolding the selection can be undone
      When the method "select" of "edit" is called with 5, 6
      And the method "bolden" of "root" is called
      And the method "undo" of "edit" is called
      And the method "html" of "root" is called
      Then the property "out" of "root" is "<p>a<span style=' font-weight:700;'>bold</span>c</p>"

    Scenario: A toolbar colours the selection
      When the method "select" of "edit" is called with 0, 1
      And the method "redden" of "root" is called
      And the method "html" of "root" is called
      Then the property "out" of "root" is "<p><span style=' color:#ff0000;'>a</span><span style=' font-weight:700;'>bold</span>c</p>"

    Scenario: A toolbar centres the paragraph
      When the method "centre" of "root" is called
      And the method "html" of "root" is called
      Then the property "out" of "root" is "<p align='center'>a<span style=' font-weight:700;'>bold</span>c</p>"

    Scenario: A centred paragraph reads as centred
      When the method "centre" of "root" is called
      And the method "format" of "root" is called
      Then the property "out" of "root" is "false,false,#000000,4,"

    Scenario: Assigning the selection's text replaces it
      When the method "select" of "edit" is called with 1, 5
      And the method "retype" of "root" is called
      And the method "html" of "root" is called
      Then the property "out" of "root" is "<p>a<span style=' font-weight:700;'>Z</span>c</p>"
