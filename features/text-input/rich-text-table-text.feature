# Sources:
#   https://doc.qt.io/qt-6/richtext-html-subset.html#table-attributes
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#getText-method
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#getFormattedText-method
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: The text of a table in a rich TextEdit
  A TextEdit writes a table back as a table, each row in a tr and each
  cell in a td, with its border, its spacing and its padding. Its plain
  text has U+FDD0 before each cell and U+FDD1 after the table. A table
  first or last in the document has a position before or after it where
  no text is, as two tables side by side have one between them, and the
  cursor there stands at the table's top left or its bottom right. The
  text is Liberation Sans at 20 pixels, where a "c" is ten pixels wide
  and a line 23 high.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 300; height: 200
        property string doc
        property string out
        function simple(html) {
          return html.replace(/^[\s\S]*<body[^>]*>\n?/, "").replace(/<\/body>[\s\S]*$/, "")
            .replace(/ style=" margin-top:0px; margin-bottom:0px; margin-left:0px; margin-right:0px;( -qt-block-indent:0; text-indent:0px;)?"/g, "")
            .split("\"").join("'").split("\n").join("|").split("<").join("[").split(">").join("]")
        }
        function hex(s) {
          return Array.from(s, c => c.charCodeAt(0) > 127 || c.charCodeAt(0) < 32 ? c.charCodeAt(0).toString(16) : c).join(" ")
        }
        function place(i) { const r = edit.positionToRectangle(i); return r.x + "," + r.y }
        function written() { out = simple(edit.text) }
        function plain() { out = edit.length + ": " + hex(edit.getText(0, edit.length)) }
        function edges() { out = place(0) + " " + place(edit.length) + " " + edit.contentHeight }
        function fragment(a, b) { out = simple(edit.getFormattedText(a, b)) }
        function selected(a, b) { edit.select(a, b); out = hex(edit.selectedText) }
        TextEdit {
          id: edit; objectName: "edit"
          textFormat: TextEdit.RichText; font.family: "Liberation Sans"; font.pixelSize: 20
          text: root.doc.split("[").join("<").split("]").join(">")
        }
      }
      """

  Rule: A table is written back as a table

    Scenario Outline: <doc> is written back as <html>
      When the property "doc" of "root" is set to "<doc>"
      And the method "written" of "root" is called
      Then the property "out" of "root" is "<html>"

      Examples:
        | doc | html |
        | [table][tr][td]a[/td][td]b[/td][/tr][/table] | [table border='0' cellspacing='2' cellpadding='0']\|[tr]\|[td]\|[p]a[/p][/td]\|[td]\|[p]b[/p][/td][/tr][/table] |
        | x[table][tr][td]a[/td][td][/td][/tr][/table]y | [p]x[/p]\|[table border='0' cellspacing='2' cellpadding='0']\|[tr]\|[td]\|[p]a[/p][/td]\|[td][/td][/tr][/table]\|[p]y[/p] |
        | x[table border='1'][tr][td]a[br]b[/td][td]c[/td][/tr][/table]y | [p]x[/p]\|[table border='1' cellspacing='2' cellpadding='0']\|[tr]\|[td]\|[p]a[br /]b[/p][/td]\|[td]\|[p]c[/p][/td][/tr][/table]\|[p]y[/p] |
        | x[table border='0' cellspacing='0'][tr][td]a[/td][/tr][tr][td]b[/td][/tr][/table] | [p]x[/p]\|[table border='0' cellspacing='0' cellpadding='0']\|[tr]\|[td]\|[p]a[/p][/td][/tr]\|[tr]\|[td]\|[p]b[/p][/td][/tr][/table] |
        | [table][tr][td]a[/td][/tr][/table][table][tr][td]b[/td][/tr][/table] | [table border='0' cellspacing='2' cellpadding='0']\|[tr]\|[td]\|[p]a[/p][/td][/tr][/table]\|[table border='0' cellspacing='2' cellpadding='0']\|[tr]\|[td]\|[p]b[/p][/td][/tr][/table] |
        | [table cellpadding='3' bgcolor='#ff0000'][tr][td bgcolor='#00ff00']a[/td][/tr][/table] | [table border='0' cellspacing='2' cellpadding='3' bgcolor='#ff0000']\|[tr]\|[td bgcolor='#00ff00']\|[p][span style=' background-color:#00ff00;']a[/span][/p][/td][/tr][/table] |

    Scenario Outline: A fragment from <from> to <to> is written as <html>
      When the property "doc" of "root" is set to "x[table][tr][td]cc[/td][td]c[/td][/tr][/table]y"
      And the method "fragment" of "root" is called with <from>, <to>
      Then the property "out" of "root" is "<html>"

      Examples:
        | from | to | html |
        | 0    | 8  | [p][!--StartFragment--]x[/p]\|[table border='0' cellspacing='2' cellpadding='0']\|[tr]\|[td]\|[p]cc[/p][/td]\|[td]\|[p]c[/p][/td][/tr][/table]\|[p]y[!--EndFragment--][/p] |
        | 2    | 4  | [p][!--StartFragment--]cc[!--EndFragment--][/p] |

  Rule: The plain text marks the cells

    Scenario Outline: The plain text of <doc> is <text>
      When the property "doc" of "root" is set to "<doc>"
      And the method "plain" of "root" is called
      Then the property "out" of "root" is "<text>"

      Examples:
        | doc | text |
        | [table][tr][td]a[/td][td]b[/td][/tr][/table] | 5: fdd0 a fdd0 b fdd1 |
        | x[table][tr][td]a[/td][td][/td][/tr][/table]y | 6: x fdd0 a fdd0 fdd1 y |
        | x[table][tr][td]a[br]b[/td][/tr][/table]y | 7: x fdd0 a 2028 b fdd1 y |
        | x[table][tr][td]a[/td][/tr][/table] | 4: x fdd0 a fdd1 |
        | [table][tr][td]a[/td][/tr][/table][table][tr][td]b[/td][/tr][/table] | 6: fdd0 a fdd1 fdd0 b fdd1 |

    Scenario: The text selected in a cell is the cell's
      When the property "doc" of "root" is set to "x[table][tr][td]ccc[/td][td]c[/td][/tr][/table]y"
      And the method "selected" of "root" is called with 2, 4
      Then the property "out" of "root" is "c c"

  Rule: A table at an end of the document has a position beside it

    Scenario Outline: The cursor beside <doc> stands at <places>
      When the property "doc" of "root" is set to "<doc>"
      And the method "edges" of "root" is called
      Then the property "out" of "root" is "<places>"

      Examples:
        | doc | places |
        | [table][tr][td]c[/td][td]cc[/td][/tr][/table] | 0,0 36,4 27 |
        | x[table border='1'][tr][td]cc[/td][/tr][tr][td]c[/td][/tr][/table] | 0,0 28,58 81 |
