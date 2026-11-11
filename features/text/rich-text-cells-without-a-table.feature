# Sources:
#   https://doc.qt.io/qt-6/richtext-html-subset.html#table-cell-attributes
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#text-prop
# Verified on Qt 6.12.0 (qml6, offscreen). The HTML a TextEdit writes is
# shown without its style attributes, double quotes as single ones and a
# line feed as "~".

@qt-parity
Feature: Table cells with no table around them
  A cell or a row that no table stands around reads as if a table started
  just before it: the cell opens a table and a row of it, and a row opens
  the table. The table ends as one does whose end tag is left out, at the
  end of a table or the end of the text, and what comes after its last
  cell goes on in that cell.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 300; height: 200
        property string out
        function bare(h) { return h.replace(/^[\s\S]*<body[^>]*>\n?/, "").replace(/<\/body>[\s\S]*$/, "").replace(/ style="[^"]*"/g, "").split("\"").join("'").split("\n").join("~") }
        function written() { out = bare(edit.text) }
        TextEdit { id: edit; objectName: "edit"; width: 280; textFormat: TextEdit.RichText }
      }
      """

  Rule: A cell or a row with no table opens one

    Scenario Outline: <case>
      When the property "text" of "edit" is set to "<html>"
      And the method "written" of "root" is called
      Then the property "out" of "root" is "<written>"

      Examples:
        | case                                     | html                                   | written                                                                                                                      |
        | A cell after a paragraph opens a table   | <p>a</p><td>b</td>                     | <p>a</p>~<table border='0' cellspacing='2' cellpadding='0'>~<tr>~<td>~<p>b</p></td></tr></table>                             |
        | A cell alone is a table                  | <td>b</td>                             | <table border='0' cellspacing='2' cellpadding='0'>~<tr>~<td>~<p>b</p></td></tr></table>                                      |
        | A cell in a paragraph ends the paragraph | <p>a<td>b</td></p>                     | <p>a</p>~<table border='0' cellspacing='2' cellpadding='0'>~<tr>~<td>~<p>b</p></td></tr></table>                             |
        | A cell left open is a table              | <p>a</p><td>b                          | <p>a</p>~<table border='0' cellspacing='2' cellpadding='0'>~<tr>~<td>~<p>b</p></td></tr></table>                             |
        | Two cells are a row of two               | <p>a</p><td>b</td><td>c</td>           | <p>a</p>~<table border='0' cellspacing='2' cellpadding='0'>~<tr>~<td>~<p>b</p></td>~<td>~<p>c</p></td></tr></table>          |
        | A row opens a table                      | <p>a</p><tr><td>b</td><td>c</td></tr>  | <p>a</p>~<table border='0' cellspacing='2' cellpadding='0'>~<tr>~<td>~<p>b</p></td>~<td>~<p>c</p></td></tr></table>          |
        | A row after a cell is the next row       | <td>b</td><tr><td>c</td></tr>          | <table border='0' cellspacing='2' cellpadding='0'>~<tr>~<td>~<p>b</p></td></tr>~<tr>~<td>~<p>c</p></td></tr></table>         |
        | A row with no cell is nothing            | <p>a</p><tr>b</tr>                     | <p>a</p>                                                                                                                     |
        | The end of a table ends it               | <p>a</p><td>b</td></table>c            | <p>a</p>~<table border='0' cellspacing='2' cellpadding='0'>~<tr>~<td>~<p>b</p></td></tr></table>~<p>c</p>                    |
        | Text after it goes on in its last cell   | a<td>b</td>c                           | <p>a</p>~<table border='0' cellspacing='2' cellpadding='0'>~<tr>~<td>~<p>b</p>~<p>c</p></td></tr></table>                    |
        | A paragraph after it is in its last cell | <p>a</p><td>b</td><p>c</p>             | <p>a</p>~<table border='0' cellspacing='2' cellpadding='0'>~<tr>~<td>~<p>b</p>~<p>c</p></td></tr></table>                    |

  Rule: So does a header cell

    A header cell's text is bold and in the middle of the cell, in a table
    or out of one.

    Scenario Outline: <case>
      When the property "text" of "edit" is set to "<html>"
      And the method "written" of "root" is called
      Then the property "out" of "root" is "<written>"

      Examples:
        | case                              | html                         | written                                                                                                                                              |
        | A header cell after a cell is one | <p>a</p><td>b</td><th>c</th> | <p>a</p>~<table border='0' cellspacing='2' cellpadding='0'>~<tr>~<td>~<p>b</p></td>~<td>~<p align='center'><span>c</span></p></td></tr></table>      |

      # Qt 6.12 opens no table for a header cell with no table around it,
      # and reads it as a paragraph in the middle; Ljós reads it as the
      # cell it is, as it does in a table.
      @qt-diverges
      Examples:
        | case                        | html                         | written                                                                                                                                                                |
        | A header cell opens a table | <p>a</p><th>b</th><td>c</td> | <p>a</p>~<table border='0' cellspacing='2' cellpadding='0'>~<tr>~<td>~<p align='center'><span>b</span></p></td>~<td>~<p>c</p></td></tr></table>                       |
