# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-textdocument.html#source-prop
#   https://doc.qt.io/qt-6/qml-qtquick-textdocument.html#status-prop
#   https://doc.qt.io/qt-6/qml-qtquick-textdocument.html#save-method
#   https://doc.qt.io/qt-6/qml-qtquick-textdocument.html#saveAs-method
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Loading and saving the document of a TextEdit
  The document of a TextEdit reads a file named by its source and writes
  the text back with save() or to another file with saveAs(). With
  textFormat AutoText the name of the file says what is in it: Markdown
  (.md), HTML (.html) or plain text. A file is read and written away from
  the thread of the user interface, so the scenarios let a moment pass
  before they look. Line breaks and paragraph separators read as bars.

  Background:
    Given the text file "notes.md":
      """
      Hello *there*

      second
      """
    And the text file "page.html":
      """
      <p>x <b>y</b></p>
      """
    And the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 300; height: 300
        property string out
        property var doc: edit.textDocument
        property int status: doc.status
        property bool modified: doc.modified
        property bool missing: doc.errorString.endsWith("missing.md does not exist")
        property string source: String(doc.source).replace(/^.*\//, "")
        function bars(s) { return s.split("\n").join("|").split("\u2029").join("|") }
        function open(name) { doc.source = Qt.resolvedUrl(name) }
        function text() { out = bars(edit.text) }
        function characters() { out = bars(edit.getText(0, edit.length)) }
        function store(name) { doc.saveAs(Qt.resolvedUrl(name)) }
        function save() { doc.save() }
        function reopen() { other.textDocument.source = edit.textDocument.source }
        function other_text() { out = bars(other.getText(0, other.length)) }
        TextEdit { id: edit; objectName: "edit"; width: 280; height: 140; textFormat: TextEdit.AutoText }
        TextEdit {
          id: other; objectName: "other"; y: 150; width: 280; height: 140
          textFormat: TextEdit.AutoText
        }
      }
      """

  Rule: Loading

    Scenario: A document has no source and no status at first
      Then the property "status" of "root" is 0
      And the property "source" of "root" is ""

    Scenario: A Markdown file is read as Markdown
      When the method "open" of "root" is called with "notes.md"
      And 100 milliseconds pass
      And the method "text" of "root" is called
      Then the property "out" of "root" is "Hello *there*||second||"
      And the property "status" of "root" is 2

    # The line break that ends the file is white space after the last
    # paragraph, and reads as one space at its end.
    Scenario: An HTML file is read as HTML
      When the method "open" of "root" is called with "page.html"
      And 100 milliseconds pass
      And the method "characters" of "root" is called
      Then the property "out" of "root" is "x y "

    Scenario: A document that was read is not modified
      When the method "open" of "root" is called with "notes.md"
      And 100 milliseconds pass
      Then the property "modified" of "root" is false
      And the property "source" of "root" is "notes.md"

    Scenario: A file that is not there is a read error
      When the method "open" of "root" is called with "missing.md"
      And 100 milliseconds pass
      Then the property "status" of "root" is 5
      And the property "missing" of "root" is true
      And the log has a warning matching "missing.md does not exist"

  Rule: Saving

    Scenario: save writes the text back and the document is saved
      When the method "open" of "root" is called with "notes.md"
      And 100 milliseconds pass
      And the method "insert" of "edit" is called with 0, "X"
      And the method "save" of "root" is called
      And 100 milliseconds pass
      Then the property "status" of "root" is 4
      And the property "modified" of "root" is false

    Scenario: What was saved is what is read again
      When the method "open" of "root" is called with "notes.md"
      And 100 milliseconds pass
      And the method "insert" of "edit" is called with 0, "X"
      And the method "save" of "root" is called
      And 100 milliseconds pass
      And the method "reopen" of "root" is called
      And 100 milliseconds pass
      And the method "other_text" of "root" is called
      Then the property "out" of "root" is "XHello there|second"

    Scenario: saveAs names the file it wrote as the source
      When the method "open" of "root" is called with "notes.md"
      And 100 milliseconds pass
      And the method "store" of "root" is called with "copy.html"
      And 100 milliseconds pass
      Then the property "source" of "root" is "copy.html"
      And the property "status" of "root" is 4

    Scenario Outline: saveAs writes <kind> that reads back as the same text
      When the method "open" of "root" is called with "notes.md"
      And 100 milliseconds pass
      And the method "store" of "root" is called with "<file>"
      And 100 milliseconds pass
      And the method "reopen" of "root" is called
      And 100 milliseconds pass
      And the method "other_text" of "root" is called
      Then the property "out" of "root" is "Hello there|second"

      Examples:
        | kind       | file      |
        | Markdown   | copy.md   |
        | HTML       | copy.html |
        | plain text | copy.txt  |

    Scenario: Saving where no directory is is a write error
      When the method "store" of "root" is called with "nowhere/at/all.md"
      And 100 milliseconds pass
      Then the property "status" of "root" is 6
