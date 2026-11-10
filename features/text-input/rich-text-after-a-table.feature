# Sources:
#   https://doc.qt.io/qt-6/richtext-html-subset.html#tables
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#getText-method
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#lineCount-prop
# Verified on Qt 6.11.2 (qml6, offscreen). The text is read with getText:
# a line break is shown as "/", the start of a new paragraph as "#", the
# start and end of a table as "=", in brackets, and after it the number
# of lines. In the HTML of a step [ and ] stand for < and >, and T for
# a table of one cell that holds "a".
# The HTML written back is shown with its zero margins left out, its
# quotes as ' and its line feeds as ~.

@qt-parity @api-QtQuick.TextEdit.textFormat
Feature: The paragraph after a table in a rich TextEdit
  Every table has a paragraph after it. What is inline after the table,
  text, an image or a line break, goes into it; a block after the table
  starts a paragraph after that one, which stays empty. An empty block
  right after the table is nothing, unless the document ends there. The
  paragraph is a line of its own and text can be typed into it, but while
  it is empty it is not written back: the table is, and then the block
  after it.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 300; height: 200
        property string html; property string out
        onHtmlChanged: edit.text = html.split("[").join("<").split("]").join(">").split("T").join("<table><tr><td>a</td></tr></table>")
        function simple(h) {
          return h.replace(/^[\s\S]*<body[^>]*>\n?/, "").replace(/<\/body>[\s\S]*$/, "")
            .replace(/ ?margin-top:0px; margin-bottom:0px; margin-left:0px; margin-right:0px;( -qt-block-indent:0; text-indent:0px;)?/g, "")
            .replace(/ style=""/g, "").split("\"").join("'").split("\n").join("~")
        }
        function shown() { return "[" + edit.getText(0, edit.length).split("\u2028").join("/").split("\u2029").join("#").replace(/[﷐﷑]/g, "=") + "]" }
        function plain() { out = shown() + " " + edit.lineCount }
        function typed() { out = edit.cursorPosition + " " + shown() }
        function written() { out = simple(edit.text) }
        function typing(at) { edit.forceActiveFocus(); edit.cursorPosition = at }
        TextEdit {
          id: edit; objectName: "edit"; width: 280; wrapMode: TextEdit.WordWrap
          textFormat: TextEdit.RichText; font.family: "Liberation Sans"; font.pixelSize: 20
        }
      }
      """

  Rule: A block after a table starts after the paragraph that follows it

    Scenario Outline: <case>
      When the property "html" of "root" is set to "<html>"
      And the method "plain" of "root" is called
      Then the property "out" of "root" is "<text>"

      Examples:
        | case                                           | html                                     | text           |
        | A paragraph after a table                      | T[p]b[/p]                                | [=a=#b] 4      |
        | A div after one                                | T[div]b[/div]                            | [=a=#b] 4      |
        | A heading                                      | T[h1]b[/h1]                              | [=a=#b] 4      |
        | A list                                         | T[ul][li]b[/li][/ul]                     | [=a=#b] 4      |
        | A pre                                          | T[pre]b[/pre]                            | [=a=#b] 4      |
        | A quote                                        | T[blockquote]b[/blockquote]              | [=a=#b] 4      |
        | Two paragraphs                                 | T[p]b[/p][p]c[/p]                        | [=a=#b#c] 5    |
        | A table after a paragraph                      | [p]x[/p]T[p]b[/p]                        | [x=a=#b] 4     |
        | A table that ends a div                        | [div]T[/div][p]b[/p]                     | [=a=#b] 4      |
        | A rule                                         | T[hr][p]b[/p]                            | [=a=##b] 5     |
        | Read with the editor's mark                    | [meta name='qrichtext' content='1' /]T[p]b[/p] | [=a=#b] 4 |

    Scenario Outline: <case>
      When the property "html" of "root" is set to "<html>"
      And the method "plain" of "root" is called
      Then the property "out" of "root" is "<text>"

      Examples:
        | case                                                | html                                                       | text           |
        | An empty paragraph after it is that paragraph       | T[p][/p][p]b[/p]                                           | [=a=#b] 4      |
        | So are two                                          | T[p][/p][p][/p][p]b[/p]                                    | [=a=#b] 4      |
        | So is an empty heading                              | T[h1][/h1][p]b[/p]                                         | [=a=#b] 4      |
        | So is an empty div                                  | T[div][/div][p]b[/p]                                       | [=a=#b] 4      |
        | An empty paragraph an editor wrote is one           | T[p style='-qt-paragraph-type:empty;'][br /][/p][p]b[/p]   | [=a=##b] 5     |
        | A paragraph of a line break is one                  | T[p][br][/p][p]b[/p]                                       | [=a=#/#b] 6    |
        | An empty paragraph that ends the document is a line | T[p][/p]                                                   | [=a=#] 4       |
        | So is an empty list                                 | T[ul][li][/li][/ul]                                        | [=a=#] 4       |
        | A table that ends the document has one line after it | T                                                         | [=a=] 3        |

    Scenario Outline: <case>
      When the property "html" of "root" is set to "<html>"
      And the method "plain" of "root" is called
      Then the property "out" of "root" is "<text>"

      Examples:
        | case                                        | html          | text           |
        | Text after a table goes into it             | Tb            | [=a=b] 3       |
        | So does bold text                           | T[b]b[/b]     | [=a=b] 3       |
        | White space before it is nothing            | T  b          | [=a=b] 3       |
        | A line break                                | T[br]b        | [=a=/b] 4      |
        | An image                                    | T[img src=x]  | [=a=￼] 3       |
        | A table after a table                       | TT            | [=a==a=] 5     |
        | A table after the paragraph after a table   | T[p]b[/p]T    | [=a=#b=a=] 6   |

  Rule: The paragraph after a table is written back and edited

    Scenario Outline: <html> is written back as <out>
      When the property "html" of "root" is set to "<html>"
      And the method "written" of "root" is called
      Then the property "out" of "root" is "<out>"

      Examples:
        | html      | out |
        | T[p]b[/p] | <table border='0' cellspacing='2' cellpadding='0'>~<tr>~<td>~<p>a</p></td></tr></table>~<p style=' margin-top:12px; margin-bottom:12px; margin-left:0px; margin-right:0px; -qt-block-indent:0; text-indent:0px;'>b</p> |
        | Tb        | <table border='0' cellspacing='2' cellpadding='0'>~<tr>~<td>~<p>a</p></td></tr></table>~<p>b</p> |
        | T[p][/p][p]b[/p] | <table border='0' cellspacing='2' cellpadding='0'>~<tr>~<td>~<p>a</p></td></tr></table>~<p style=' margin-top:12px; margin-bottom:12px; margin-left:0px; margin-right:0px; -qt-block-indent:0; text-indent:0px;'>b</p> |
        | T[p][/p]  | <table border='0' cellspacing='2' cellpadding='0'>~<tr>~<td>~<p>a</p></td></tr></table>~<p style='-qt-paragraph-type:empty;'><br /></p> |
        | T[ul][li][/li][/ul] | <table border='0' cellspacing='2' cellpadding='0'>~<tr>~<td>~<p>a</p></td></tr></table>~<p style='-qt-paragraph-type:empty;'><br /></p> |
        | T         | <table border='0' cellspacing='2' cellpadding='0'>~<tr>~<td>~<p>a</p></td></tr></table> |
        | T[p style='-qt-paragraph-type:empty;'][br /][/p][p]b[/p] | <table border='0' cellspacing='2' cellpadding='0'>~<tr>~<td>~<p>a</p></td></tr></table>~<p style='-qt-paragraph-type:empty; margin-top:12px; margin-bottom:12px; margin-left:0px; margin-right:0px; -qt-block-indent:0; text-indent:0px;'><br /></p>~<p style=' margin-top:12px; margin-bottom:12px; margin-left:0px; margin-right:0px; -qt-block-indent:0; text-indent:0px;'>b</p> |

    Scenario Outline: Typing <keys> at <at> is written back as <out>
      When the property "html" of "root" is set to "T[p]b[/p]"
      And the method "typing" of "root" is called with <at>
      And the user types "<keys>"
      And the method "written" of "root" is called
      Then the property "out" of "root" is "<out>"

      Examples:
        | at | keys | out |
        | 3  | x    | <table border='0' cellspacing='2' cellpadding='0'>~<tr>~<td>~<p>a</p></td></tr></table>~<p>x</p>~<p style=' margin-top:12px; margin-bottom:12px; margin-left:0px; margin-right:0px; -qt-block-indent:0; text-indent:0px;'>b</p> |

    Scenario Outline: Typing <keys> at <at> gives <text>
      When the property "html" of "root" is set to "T[p]b[/p]"
      And the method "typing" of "root" is called with <at>
      And the user types "<keys>"
      And the method "typed" of "root" is called
      Then the property "out" of "root" is "<text>"

      Examples:
        | at | keys | text        |
        | 3  | x    | 4 [=a=x#b]  |
        | 4  | x    | 5 [=a=#xb]  |

    Scenario Outline: <keys> at <at> gives <text>
      When the property "html" of "root" is set to "T[p]b[/p]"
      And the method "typing" of "root" is called with <at>
      And the user presses "<keys>"
      And the method "typed" of "root" is called
      Then the property "out" of "root" is "<text>"

      Examples:
        | at | keys      | text        |
        | 4  | Backspace | 3 [=a=b]    |
        | 3  | Backspace | 3 [=a=#b]   |
        | 3  | Delete    | 3 [=a=b]    |
        | 2  | Delete    | 2 [=a=#b]   |
        | 3  | Return    | 4 [=a=##b]  |

    Scenario Outline: <keys> at <at> is written back as <out>
      When the property "html" of "root" is set to "T[p]b[/p]"
      And the method "typing" of "root" is called with <at>
      And the user presses "<keys>"
      And the method "written" of "root" is called
      Then the property "out" of "root" is "<out>"

      Examples:
        | at | keys      | out |
        | 4  | Backspace | <table border='0' cellspacing='2' cellpadding='0'>~<tr>~<td>~<p>a</p></td></tr></table>~<p style=' margin-top:12px; margin-bottom:12px; margin-left:0px; margin-right:0px; -qt-block-indent:0; text-indent:0px;'>b</p> |
