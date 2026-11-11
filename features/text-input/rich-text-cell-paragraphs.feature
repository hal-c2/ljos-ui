# Sources:
#   https://doc.qt.io/qt-6/richtext-html-subset.html#table-cell-attributes
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#getText-method
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#getFormattedText-method
# Verified on Qt 6.11.2 (qml6, offscreen). The text is read with getText:
# a line break is shown as "/", the start of a new paragraph as "#", the
# start and end of a table as "=", in brackets. In the HTML of a step a
# "~" stands for a line feed, [ and ] for < and >, and LONG for a line
# too long for the item.
# The HTML written back is shown with its zero margins left out, its
# quotes as ' and its line feeds as ~.

@qt-parity @api-QtQuick.TextEdit.textFormat
Feature: Paragraphs in the cells of a rich TextEdit
  A cell of a table holds paragraphs of its own. A block that starts
  after text in the cell starts one, and so does text after a block's
  end, but not after a div's: the text goes on in the div's paragraph. A
  block that starts a cell, or follows an empty one, starts none. A line
  feed that is kept starts one. Between two paragraphs of a cell the
  plain text has U+2029, as between paragraphs outside a table. Return
  in a cell adds a paragraph to it, Backspace at the start of one joins
  it to the one before and Delete at the end of one joins the next to
  it. A removal from inside a paragraph keeps that paragraph's format;
  one from the start of a paragraph keeps the format of the paragraph it
  ends in. Deleting a
  word never takes the cursor out of its table: from one cell into
  another it empties both, and from outside into a table it takes the
  whole table and leaves the paragraphs on either side apart. Every
  paragraph of a cell is written back as a `<p>` of its own, and each
  wraps as it says. What is copied has a line feed between the
  paragraphs of a cell and at a line break.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 300; height: 200
        property string html; property string out
        onHtmlChanged: edit.text = html.split("[").join("<").split("]").join(">").split("~").join("\n").split("LONG").join("aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj kkkk llll")
        function simple(h) {
          return h.replace(/^[\s\S]*<body[^>]*>\n?/, "").replace(/<\/body>[\s\S]*$/, "")
            .replace(/ ?margin-top:0px; margin-bottom:0px; margin-left:0px; margin-right:0px;( -qt-block-indent:0; text-indent:0px;)?/g, "")
            .replace(/ style=""/g, "").split("\"").join("'").split("\n").join("~")
        }
        function shown() { return "[" + edit.getText(0, edit.length).split(" ").join("/").split(" ").join("#").replace(/[﷐﷑]/g, "=").split("\u00a0").join("_") + "]" }
        function plain() { out = shown() }
        function typed() { out = edit.cursorPosition + " " + shown() }
        function written() { out = simple(edit.text) }
        function fragment(a, b) { out = simple(edit.getFormattedText(a, b)) }
        function selected(a, b) { edit.select(a, b); out = Array.from(edit.selectedText, c => c.charCodeAt(0).toString(16)).join(" ") }
        function copied(a, b) { edit.select(a, b); edit.copy(); src.text = ""; src.paste(); out = Array.from(src.text, c => c.charCodeAt(0).toString(16)).join(" ") }
        function wide() { out = edit.contentWidth > edit.width ? "wider than the item" : "within the item" }
        function typing(at) { edit.forceActiveFocus(); edit.cursorPosition = at }
        TextEdit { id: src; y: 150 }
        TextEdit {
          id: edit; objectName: "edit"; width: 280; wrapMode: TextEdit.WordWrap
          textFormat: TextEdit.RichText; font.family: "Liberation Sans"; font.pixelSize: 20
        }
      }
      """

  Rule: A cell holds paragraphs of its own

    Scenario Outline: <case>
      When the property "html" of "root" is set to "<html>"
      And the method "plain" of "root" is called
      Then the property "out" of "root" is "<text>"

      Examples:
        | case                                            | html                                                                                  | text      |
        | Two paragraphs in a cell                        | [table][tr][td][p]a[/p][p]b[/p][/td][td]z[/td][/tr][/table]                           | [=a#b=z=] |
        | In the cell after the first too                 | [table][tr][td]a[/td][td][p]b[/p][p]c[/p][/td][/tr][/table]                           | [=a=b#c=] |
        | A paragraph after text in a cell                | [table][tr][td]a[p]b[/p][/td][/tr][/table]                                            | [=a#b=]   |
        | Text after a heading's end                      | [table][tr][td][h1]a[/h1]b[/td][/tr][/table]                                          | [=a#b=]   |
        | Text after a list's end                         | [table][tr][td][ul][li]a[/li][/ul]b[/td][/tr][/table]                                 | [=a#b=]   |
        | A line break after a paragraph's end            | [table][tr][td][p]a[/p][br][/td][/tr][/table]                                         | [=a#/=]   |
        | A paragraph after a line break                  | [table][tr][td]a[br][p]b[/p][/td][/tr][/table]                                        | [=a/#b=]  |
        | An empty paragraph after text                   | [table][tr][td]a[p][/p][/td][/tr][/table]                                             | [=a#=]    |
        | No paragraph after an empty one at the start    | [table][tr][td][p][/p][p]b[/p][/td][/tr][/table]                                      | [=b=]     |
        | Text after a div's end goes on in its paragraph | [table][tr][td][div]a[/div]b[/td][/tr][/table]                                        | [=ab=]    |
        | A line feed in a pre in a cell                  | [table][tr][td][pre]a~b[/pre][/td][/tr][/table]                                       | [=a#b=]   |
        | A line feed kept in a pre-wrap cell             | [table][tr][td style='white-space:pre-wrap']a~b[/td][/tr][/table]                     | [=a#b=]   |
        | One right after a paragraph's start is not      | [table][tr][td style='white-space:pre-wrap']a[p]~b[/p][/td][/tr][/table]              | [=a#b=]   |
        | Nor one right after a paragraph's end           | [table][tr][td style='white-space:pre-wrap'][p]a[/p]~[p]b[/p][/td][/tr][/table]       | [=a#b=]   |

  Rule: What comes after a cell ends is a paragraph of that cell

    Text, a block or a line break between the end of a cell and the next
    one goes on in the cell before as a paragraph of its own; white space
    alone there is nothing. Before a row's first cell nothing is kept.
    Measured on Qt 6.12.0.

    Scenario Outline: <case>
      When the property "html" of "root" is set to "<html>"
      And the method "plain" of "root" is called
      Then the property "out" of "root" is "<text>"

      Examples:
        | case                                     | html                                                                   | text       |
        | Text after a row's end                   | [table][tr][td]b[/td][/tr]c[/table]                                    | [=b#c=]    |
        | Text after a cell's end                  | [table][tr][td]b[/td]  c[/tr][/table]                                  | [=b#c=]    |
        | A paragraph and text after a cell's end  | [table][tr][td]b[/td][p]c[/p]d[/tr][/table]                            | [=b#c#d=]  |
        | Text in a span is one paragraph          | [table][tr][td]b[/td][b]c[/b]d[td]e[/td][/tr][/table]                  | [=b#cd=e=] |
        | A line break after a cell's end          | [table][tr][td]b[/td][br]c[/tr][/table]                                | [=b#/c=]   |
        | Text after an empty cell fills it        | [table][tr][td][/td]c[/tr][/table]                                     | [=c=]      |
        | White space between cells is nothing     | [table][tr][td]b[/td] [span] [/span] [td]e[/td][/tr][/table]           | [=b=e=]    |
        | Text before a row's first cell is gone   | [table][tr][td]b[/td][/tr][tr]c[td]d[/td][/tr][/table]                 | [=b=d=]    |
        | Text before the first row is gone        | [table]c[tr][td]b[/td][/tr][/table]                                    | [=b=]      |

  Rule: Return in a cell adds a paragraph to it

    Scenario Outline: <keys> at <at> in a cell of "ab" gives <text>
      When the property "html" of "root" is set to "[table][tr][td]ab[/td][td]z[/td][/tr][/table]"
      And the method "typing" of "root" is called with <at>
      And the user presses "<keys>"
      And the method "typed" of "root" is called
      Then the property "out" of "root" is "<text>"

      Examples:
        | at | keys   | text         |
        | 2  | Return | 3 [=a#b=z=]  |
        | 1  | Return | 2 [=#ab=z=]  |
        | 3  | Return | 4 [=ab#=z=]  |

    Scenario: Return twice adds two
      When the property "html" of "root" is set to "[table][tr][td]ab[/td][td]z[/td][/tr][/table]"
      And the method "typing" of "root" is called with 2
      And the user presses "Return"
      And the user presses "Return"
      And the method "typed" of "root" is called
      Then the property "out" of "root" is "4 [=a##b=z=]"

    Scenario: Return at the end of a paragraph of a cell adds one after it
      When the property "html" of "root" is set to "[table][tr][td][p]a[/p][p]b[/p][/td][td]z[/td][/tr][/table]"
      And the method "typing" of "root" is called with 2
      And the user presses "Return"
      And the method "typed" of "root" is called
      Then the property "out" of "root" is "3 [=a##b=z=]"

    Scenario Outline: Return at <at> is written back as <html>
      When the property "html" of "root" is set to "<doc>"
      And the method "typing" of "root" is called with <at>
      And the user presses "Return"
      And the method "written" of "root" is called
      Then the property "out" of "root" is "<html>"

      Examples:
        | doc                                              | at | html                                                                                                                                     |
        | [table][tr][td]ab[/td][td]z[/td][/tr][/table]    | 2  | <table border='0' cellspacing='2' cellpadding='0'>~<tr>~<td>~<p>a</p>~<p>b</p></td>~<td>~<p>z</p></td></tr></table>                         |
        | [table][tr][td]ab[/td][td]z[/td][/tr][/table]    | 3  | <table border='0' cellspacing='2' cellpadding='0'>~<tr>~<td>~<p>ab</p>~<p style='-qt-paragraph-type:empty;'><br /></p></td>~<td>~<p>z</p></td></tr></table> |
        | [table][tr][td]ab[/td][td]z[/td][/tr][/table]    | 1  | <table border='0' cellspacing='2' cellpadding='0'>~<tr>~<td>~<p>ab</p></td>~<td>~<p>z</p></td></tr></table>                                 |
        | x[table][tr][td]ab[/td][td]z[/td][/tr][/table]   | 3  | <p>x</p>~<table border='0' cellspacing='2' cellpadding='0'>~<tr>~<td>~<p>a</p>~<p>b</p></td>~<td>~<p>z</p></td></tr></table>                |

    Scenario: The empty paragraph between two is written as empty
      When the property "html" of "root" is set to "[table][tr][td]ab[/td][td]z[/td][/tr][/table]"
      And the method "typing" of "root" is called with 2
      And the user presses "Return"
      And the user presses "Return"
      And the method "written" of "root" is called
      Then the property "out" of "root" is "<table border='0' cellspacing='2' cellpadding='0'>~<tr>~<td>~<p>a</p>~<p style='-qt-paragraph-type:empty;'><br /></p>~<p>b</p></td>~<td>~<p>z</p></td></tr></table>"

    Scenario Outline: The text from <from> to <to> after Return is <out>
      When the property "html" of "root" is set to "[table][tr][td]ab[/td][td]z[/td][/tr][/table]"
      And the method "typing" of "root" is called with 2
      And the user presses "Return"
      And the method "<how>" of "root" is called with <from>, <to>
      Then the property "out" of "root" is "<out>"

      Examples:
        | how      | from | to | out                                                                                                                   |
        | selected | 1    | 4  | 61 2029 62                                                                                                            |
        | fragment | 1    | 4  | <p><!--StartFragment-->a</p>~<p>b<!--EndFragment--></p>                                                               |
        | fragment | 0    | 6  | <table border='0' cellspacing='2' cellpadding='0'>~<tr>~<td>~<p>a</p>~<p>b</p></td>~<td>~<p>z</p></td></tr></table> |

    Scenario Outline: A paragraph Return makes in a nowrap cell does not wrap, at <at>
      When the property "html" of "root" is set to "[table][tr][td style='white-space:nowrap']xLONG[/td][/tr][/table]"
      And the method "typing" of "root" is called with <at>
      And the user presses "Return"
      And the method "wide" of "root" is called
      Then the property "out" of "root" is "wider than the item"

      Examples:
        | at |
        | 2  |
        | 6  |

    Scenario: One Return makes in a cell that wraps wraps
      When the property "html" of "root" is set to "[table][tr][td]xLONG[/td][/tr][/table]"
      And the method "typing" of "root" is called with 6
      And the user presses "Return"
      And the method "wide" of "root" is called
      Then the property "out" of "root" is "within the item"

  Rule: Backspace and Delete join the paragraphs of a cell, never its cells

    Scenario Outline: <keys> at <at> gives <text>
      When the property "html" of "root" is set to "[table][tr][td][p]a[/p][p]b[/p][/td][td]z[/td][/tr][/table]"
      And the method "typing" of "root" is called with <at>
      And the user presses "<keys>"
      And the method "typed" of "root" is called
      Then the property "out" of "root" is "<text>"

      Examples:
        | at | keys      | text        |
        | 3  | Backspace | 2 [=ab=z=]  |
        | 2  | Delete    | 2 [=ab=z=]  |
        | 1  | Backspace | 1 [=a#b=z=] |
        | 4  | Delete    | 4 [=a#b=z=] |

    Scenario: Backspace after Return takes the paragraph back out
      When the property "html" of "root" is set to "[table][tr][td]ab[/td][td]z[/td][/tr][/table]"
      And the method "typing" of "root" is called with 2
      And the user presses "Return"
      And the user presses "Backspace"
      And the method "written" of "root" is called
      Then the property "out" of "root" is "<table border='0' cellspacing='2' cellpadding='0'>~<tr>~<td>~<p>ab</p></td>~<td>~<p>z</p></td></tr></table>"

    Scenario: The paragraph joined keeps the first one's nowrap
      When the property "html" of "root" is set to "[table][tr][td][p style='white-space:nowrap']x[/p][p]LONG[/p][/td][/tr][/table]"
      And the method "typing" of "root" is called with 3
      And the user presses "Backspace"
      And the method "wide" of "root" is called
      Then the property "out" of "root" is "wider than the item"

    Scenario Outline: Removing <from> to <to> of <doc> leaves one that is <width>
      When the property "html" of "root" is set to "<doc>"
      And the method "remove" of "edit" is called with <from>, <to>
      And the method "wide" of "root" is called
      Then the property "out" of "root" is "<width>"

      # From the start of a paragraph the paragraph the removal ends in
      # keeps its format; from inside one, the paragraph it starts in keeps
      # its own.
      Examples:
        | doc                                                                        | from | to | width               |
        | [table][tr][td][p style='white-space:nowrap']x[/p][p]LONG[/p][/td][/tr][/table]  | 1    | 3  | within the item     |
        | [table][tr][td][p]x[/p][p style='white-space:nowrap']LONG[/p][/td][/tr][/table]  | 1    | 3  | wider than the item |
        | [table][tr][td][p style='white-space:nowrap']xy[/p][p]LONG[/p][/td][/tr][/table] | 2    | 4  | wider than the item |
        | [table][tr][td][p]a[/p][p style='white-space:nowrap']x[/p][p]LONG[/p][/td][/tr][/table] | 3 | 5 | within the item |

      # The spaces of a paragraph that does not wrap are read as
      # non-breaking ones, so its text stays on one line in the paragraph
      # it goes on in, whose format wraps.
      Examples:
        | doc                                                                        | from | to | width               |
        | [table][tr][td][p]xy[/p][p style='white-space:nowrap']LONG[/p][/td][/tr][/table] | 2    | 4  | wider than the item |

  Rule: Deleting a word never leaves a cell's table, but empties the cells it reaches

    Scenario Outline: <keys> at <at> of <doc> gives <text>
      When the property "html" of "root" is set to "<doc>"
      And the method "typing" of "root" is called with <at>
      And the user presses "<keys>"
      And the method "typed" of "root" is called
      Then the property "out" of "root" is "<text>"

      Examples:
        | doc                                                                       | at | keys           | text                       |
        | [table][tr][td]ab cd[/td][td]ef[/td][/tr][/table]                         | 1  | Ctrl+Backspace | 1 [=ab cd=ef=]             |
        | [table][tr][td]ab cd[/td][td]ef[/td][/tr][/table]                         | 9  | Ctrl+Delete    | 9 [=ab cd=ef=]             |
        | [table][tr][td]ab cd[/td][td]ef[/td][/tr][/table]                         | 4  | Ctrl+Backspace | 1 [=cd=ef=]                |
        | [table][tr][td]ab cd[/td][td]ef[/td][/tr][/table]                         | 6  | Ctrl+Delete    | 2 [===]                    |
        | [table][tr][td]ab cd[/td][td]ef[/td][/tr][/table]                         | 7  | Ctrl+Backspace | 1 [===]                    |
        | [table][tr][td]ab[/td][/tr][tr][td]cd[/td][/tr][/table]                   | 4  | Ctrl+Backspace | 1 [===]                    |
        | [p]xy[/p][table][tr][td]ab[/td][/tr][/table]                              | 2  | Ctrl+Delete    | 2 [xy]                     |
        | [p]xy[/p][table][tr][td]ab[/td][/tr][/table][p]cd[/p]                     | 2  | Ctrl+Delete    | 2 [xy#cd]                  |
        | [p]xy[/p][table][tr][td]ab[/td][/tr][/table][p]cd[/p]                     | 6  | Ctrl+Backspace | 2 [xy#cd]                  |
        | [p]xy zw[/p][table][tr][td]ab cd[/td][td]ef[/td][/tr][/table][p]gh ij[/p] | 5  | Ctrl+Delete    | 5 [xy zw#gh ij]            |
        | [p]xy zw[/p][table][tr][td]ab cd[/td][td]ef[/td][/tr][/table][p]gh ij[/p] | 15 | Ctrl+Backspace | 5 [xy zw#gh ij]            |
        | [p]xy[/p][table][tr][td]ab[/td][/tr][/table][p]cd[/p]                     | 3  | Ctrl+Backspace | 3 [xy=ab=#cd]              |
        | [p]xy[/p][table][tr][td]ab[/td][/tr][/table][p]cd[/p]                     | 5  | Ctrl+Delete    | 5 [xy=ab=#cd]              |
        | [p]xy[/p][table][tr][td]ab[/td][/tr][/table][p]cd[/p]                     | 7  | Ctrl+Backspace | 6 [xy=ab=cd]               |
        | [p]xy[/p][table][tr][td]ab[/td][/tr][/table][p]cd[/p]                     | 6  | Ctrl+Delete    | 6 [xy=ab=cd]               |
        | [p]xy zw[/p][table][tr][td]ab cd[/td][td]ef[/td][/tr][/table][p]gh ij[/p] | 6  | Ctrl+Backspace | 6 [xy zw=ab cd=ef=#gh ij]  |
        | [p]xy zw[/p][table][tr][td]ab cd[/td][td]ef[/td][/tr][/table][p]gh ij[/p] | 14 | Ctrl+Delete    | 14 [xy zw=ab cd=ef=#gh ij] |
        | [p]xy zw[/p][table][tr][td]ab cd[/td][td]ef[/td][/tr][/table][p]gh ij[/p] | 9  | Ctrl+Backspace | 6 [xy zw=cd=ef=#gh ij]     |
        | [p]xy zw[/p][table][tr][td]ab cd[/td][td]ef[/td][/tr][/table][p]gh ij[/p] | 11 | Ctrl+Delete    | 7 [xy zw===#gh ij]         |
        | [p]xy zw[/p][table][tr][td]ab cd[/td][td]ef[/td][/tr][/table][p]gh ij[/p] | 12 | Ctrl+Backspace | 6 [xy zw===#gh ij]         |
        | [p]xy&nbsp;[/p][table][tr][td]ab[/td][/tr][/table][p]cd[/p]               | 2  | Ctrl+Delete    | 2 [xy=ab=#cd]              |
        | [p]xy&nbsp;[/p][table][tr][td]ab[/td][/tr][/table][p]cd[/p]               | 3  | Ctrl+Delete    | 3 [xy_#cd]                 |
        | [p]xy[/p][table][tr][td]ab[/td][/tr][/table][p]&nbsp;cd[/p]               | 7  | Ctrl+Backspace | 6 [xy=ab=_cd]              |

  Rule: Deleting a word stops at the start and the end of a paragraph

    Scenario Outline: <keys> at <at> of <doc> stops at the paragraph's edge
      When the property "html" of "root" is set to "<doc>"
      And the method "typing" of "root" is called with <at>
      And the user presses "<keys>"
      And the method "typed" of "root" is called
      Then the property "out" of "root" is "<text>"

      Examples:
        | doc                                                           | at | keys           | text           |
        | [p]xy[/p][p]&nbsp;cd[/p]                                      | 4  | Ctrl+Backspace | 3 [xy#cd]      |
        | [p]xy[/p][p]&nbsp;cd[/p]                                      | 2  | Ctrl+Delete    | 2 [xy_cd]      |
        | [p]xy[/p][table][tr][td]ab[/td][/tr][/table][p]&nbsp;cd[/p]   | 8  | Ctrl+Backspace | 7 [xy=ab=#cd]  |
        | [table][tr][td]ab[p]&nbsp;cd[/p][/td][/tr][/table]            | 5  | Ctrl+Backspace | 4 [=ab#cd=]    |
        | [table][tr][td]ab[p]&nbsp;cd[/p][/td][/tr][/table]            | 3  | Ctrl+Delete    | 3 [=ab_cd=]    |

  Rule: What is copied has a line feed between the paragraphs of a cell

    Scenario Outline: Copying <from> to <to> of <doc> copies <out>
      When the property "html" of "root" is set to "<doc>"
      And the method "copied" of "root" is called with <from>, <to>
      Then the property "out" of "root" is "<out>"

      Examples:
        | doc                                                          | from | to | out              |
        | [table][tr][td][p]a[/p][p]b[/p][/td][td]c[/td][/tr][/table]  | 1    | 5  | a 61 a 62 a 63 a |
        | [table][tr][td][p]a[/p][p]b[/p][/td][td]c[/td][/tr][/table]  | 1    | 4  | 61 a 62          |
        | [table][tr][td]a[br]b[/td][td]c[/td][/tr][/table]            | 1    | 5  | a 61 a 62 a 63 a |
        | [table][tr][td]a[br]b[/td][td]c[/td][/tr][/table]            | 1    | 4  | 61 a 62          |
        | a[br]b                                                       | 0    | 3  | 61 a 62          |

  Rule: Each paragraph of a cell wraps as it says

    Scenario Outline: <case>
      When the property "html" of "root" is set to "<html>"
      And the method "wide" of "root" is called
      Then the property "out" of "root" is "<width>"

      Examples:
        | case                                       | html                                                                                         | width               |
        | A paragraph after a nowrap one wraps       | [table][tr][td][p style='white-space:nowrap']x[/p][p]LONG[/p][/td][/tr][/table]              | within the item     |
        | A nowrap one after one that wraps does not | [table][tr][td][p]x[/p][p style='white-space:nowrap']LONG[/p][/td][/tr][/table]              | wider than the item |
