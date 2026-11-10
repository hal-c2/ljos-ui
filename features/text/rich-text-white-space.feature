# Sources:
#   https://doc.qt.io/qt-6/richtext-html-subset.html#css-properties
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#getText-method
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#text-prop
# Verified on Qt 6.11.2 (qml6, offscreen). What becomes of the white space
# is read with getText: a line break is shown as "/", the start of a new
# paragraph as "#", a space that does not break as "_", the start and end
# of a table as "=", and the text is in brackets so that its spaces show.
# A "~" in the HTML of a step stands for a line feed.

@qt-parity @api-QtQuick.TextEdit.textFormat
Feature: White space in rich text
  The CSS property `white-space` says what becomes of the spaces and line
  feeds in the text of an element. `normal` makes each run of them one
  space, and leaves out the ones that start a line; `nowrap` does too, with
  a space that does not break, so the line breaks only where it is told
  to. `pre` and
  `pre-wrap` keep every space, and a line feed starts a new paragraph.
  `pre-line` collapses spaces and keeps line feeds. An element's own style
  or a rule of a style sheet can say it, and what is inside inherits it.
  A paragraph that is `pre` or `nowrap`, and a `<pre>`, breaks its lines
  only where it is told to, however narrow the item, and so does a table
  cell.
  The HTML a TextEdit writes says `white-space: pre-wrap` for its
  paragraphs, so its spaces read back as they were.
  White space between blocks is at the end of the block before it.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 300; height: 200
        property string html; property string out
        onHtmlChanged: edit.text = html.split("~").join("\n")
        function show(e) {
          const t = e.getText(0, e.length)
          out = "[" + t.split("\u2028").join("/").split("\u2029").join("#").split("\u00a0").join("_").replace(/[\ufdd0\ufdd1]/g, "=") + "]"
        }
        function plain() { show(edit) }
        function spaceBold() { const h = edit.getFormattedText(1, 2).replace(/[\s\S]*<body[^>]*>/, ""); out = /font-weight:\s*(bold|[6-9]00)/.test(h) ? "bold" : "not bold" }
        function lines() { out = edit.lineCount > 1 ? "wraps" : "one line" }
        function anywhere() { edit.wrapMode = TextEdit.Wrap; lines() }
        function measure(e) { out = e.contentWidth > e.width ? "wider than the item" : "within the item" }
        function wide() { measure(edit) }
        function again() { other.text = edit.text; show(other) }
        function againWide() { other.text = edit.text; measure(other) }
        property string markdown
        onMarkdownChanged: md.text = markdown.split("~").join("\n")
        function markdownWide() { measure(md) }
        TextEdit { id: edit; objectName: "edit"; width: 280; focus: true; wrapMode: TextEdit.WordWrap; textFormat: TextEdit.RichText }
        TextEdit { id: other; width: 280; wrapMode: TextEdit.WordWrap; textFormat: TextEdit.RichText }
        TextEdit { id: md; width: 280; wrapMode: TextEdit.Wrap; textFormat: TextEdit.MarkdownText }
        Text { id: label; objectName: "label"; width: 280; wrapMode: Text.WordWrap; textFormat: Text.RichText; function wide() { text = root.html.split("~").join("\n"); root.measure(label) } }
      }
      """

  Rule: pre and pre-wrap keep every space

    Scenario Outline: <case>
      When the property "html" of "root" is set to "<html>"
      And the method "plain" of "root" is called
      Then the property "out" of "root" is "<text>"

      Examples:
        | case                                          | html                                                                                 | text                    |
        | Indentation after a line break is kept        | <p style='margin:0;white-space:pre-wrap'>def f():<br/>    return 1</p>               | [def f():/    return 1] |
        | pre keeps a run of spaces                     | <p style='white-space:pre'>a   b</p>                                                 | [a   b]                 |
        | pre-wrap keeps spaces at both ends            | <p style='white-space:pre-wrap'>  a   b  </p>                                        | [  a   b  ]             |
        | The space before a line break is kept         | <p style='white-space:pre-wrap'>a  <br>  b</p>                                       | [a  /  b]               |
        | A rule for p of a style sheet says it         | <style>p, li { white-space: pre-wrap; }</style><p>  a   b</p>                        | [  a   b]               |
        | A rule for li of a style sheet says it        | <style>p, li { white-space: pre-wrap; }</style><ul><li>  a   b</li></ul>             | [  a   b]               |
        | What is inside the paragraph inherits it      | <p style='white-space:pre-wrap'><b>  a</b>   b</p>                                   | [  a   b]               |
        | A paragraph inherits it from a div around it  | <div style='white-space:pre-wrap'><p>a   b</p></div>                                 | [a   b]                 |
        | An element inside may say normal              | <p style='white-space:pre-wrap'><span style='white-space:normal'>a   b</span>  c</p> | [a b  c]                |
        | A span may say it for its own text            | <span style='white-space:pre-wrap'>a   b</span>  c                                   | [a   b c]               |
        | The element's own style wins over a rule      | <style>p { white-space: pre-wrap }</style><p style='white-space:normal'>a   b</p>    | [a b]                   |
        | A value nobody knows leaves what is inherited | <div style='white-space:pre-wrap'><p style='white-space:bogus'>a   b</p></div>       | [a   b]                 |

  Rule: Where line feeds are kept, one starts a new paragraph

    Scenario Outline: <case>
      When the property "html" of "root" is set to "<html>"
      And the method "plain" of "root" is called
      Then the property "out" of "root" is "<text>"

      Examples:
        | case                                                   | html                                              | text     |
        | A line feed in pre-wrap starts a paragraph             | <p style='white-space:pre-wrap'>a  ~b</p>         | [a  #b]  |
        | An empty line is an empty paragraph                    | <p style='white-space:pre-wrap'>a~~b</p>          | [a##b]   |
        | A line feed right after the start tag is not text      | <p style='white-space:pre-wrap'>~~  a</p>         | [#  a]   |
        | A line feed at the end makes no paragraph              | <p style='white-space:pre-wrap'>a~</p>            | [a]      |
        | So it is in pre                                        | <pre>~  a</pre>                                   | [  a]    |
        | So it is in a table cell                               | <table><tr><td><pre>~code</pre></td></tr></table> | [=code=] |
        | pre-line keeps the line feed, not the spaces around it | <p style='white-space:pre-line'>  a  ~  b</p>     | [a#b]    |

  Rule: normal, nowrap and pre-line collapse spaces

    Scenario Outline: <case>
      When the property "html" of "root" is set to "<html>"
      And the method "plain" of "root" is called
      Then the property "out" of "root" is "<text>"

      Examples:
        | case                                        | html                                        | text  |
        | Without white-space spaces collapse         | <p>  a   b</p>                              | [a b] |
        | normal collapses                            | <p style='white-space:normal'>  a   b</p>   | [a b] |
        | nowrap collapses to one that does not break | <p style='white-space:nowrap'>  a   b</p>   | [a_b] |
        | pre-line collapses                          | <p style='white-space:pre-line'>  a   b</p> | [a b] |
        | normal on a pre collapses                   | <pre style='white-space:normal'>a   b</pre> | [a b] |

  Rule: nowrap does not wrap at its spaces

    Its spaces do not break, so a line wrapped at words stays whole.

    Scenario Outline: <case>
      When the property "html" of "root" is set to "<html>"
      And the method "lines" of "root" is called
      Then the property "out" of "root" is "<lines>"

      Examples:
        | case                                | html                                                                                          | lines    |
        | A long normal paragraph wraps       | <p style='white-space:normal'>aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj kkkk llll</p> | wraps    |
        | A long nowrap paragraph stays whole | <p style='white-space:nowrap'>aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj kkkk llll</p> | one line |

  Rule: A pre or nowrap paragraph, and a pre element, break lines only where told

    It does not wrap at its spaces, nor inside a word too long for the
    line. The paragraph is as wide as its longest line, wider than the item
    if it must. It is the paragraph that says it: a span inside cannot.

    Scenario Outline: <case>
      When the property "html" of "root" is set to "<html>"
      And the method "anywhere" of "root" is called
      Then the property "out" of "root" is "<lines>"

      Examples:
        | case                                           | html                                                                                                                                  | lines    |
        | A long normal paragraph wraps                  | <p>aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj kkkk llll</p>                                                                    | wraps    |
        | A word too long for the line breaks inside     | <p>aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa</p>                                                                 | wraps    |
        | A pre paragraph does not wrap                  | <p style='white-space:pre'>aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj kkkk llll</p>                                            | one line |
        | A nowrap paragraph does not wrap               | <p style='white-space:nowrap'>aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj kkkk llll</p>                                         | one line |
        | Nor does a long word in it break               | <p style='white-space:nowrap'>aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa</p>                                      | one line |
        | A pre element does not wrap                    | <pre>aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj kkkk llll</pre>                                                                | one line |
        | pre-wrap wraps                                 | <p style='white-space:pre-wrap'>aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj kkkk llll</p>                                       | wraps    |
        | pre-line wraps                                 | <p style='white-space:pre-line'>aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj kkkk llll</p>                                       | wraps    |
        | A paragraph inherits it from a div around it   | <div style='white-space:nowrap'><p>aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj kkkk llll</p></div>                              | one line |
        | A list item may say it                         | <ul><li style='white-space:nowrap'>aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj kkkk llll</li></ul>                              | one line |
        | A heading may say it                           | <h3 style='white-space:nowrap'>aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj kkkk llll</h3>                                       | one line |
        | A span cannot keep its paragraph from wrapping | <p><span style='white-space:nowrap'>aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj kkkk llll</span></p>                            | wraps    |
        | Nor can a span make it wrap                    | <p style='white-space:nowrap'><span style='white-space:normal'>aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj kkkk llll</span></p> | one line |

    Scenario Outline: <case>
      When the property "html" of "root" is set to "<html>"
      And the method "wide" of "root" is called
      Then the property "out" of "root" is "<width>"

      Examples:
        | case                                      | html                                                                                            | width               |
        | A nowrap paragraph is wider than the item | <p style='white-space:nowrap'>aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj kkkk llll</p>   | wider than the item |
        | A pre-wrap paragraph is not               | <p style='white-space:pre-wrap'>aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj kkkk llll</p> | within the item     |

    Scenario: A line break still breaks a nowrap paragraph
      When the property "html" of "root" is set to "<p style='white-space:nowrap'>aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj kkkk llll<br>x</p>"
      Then the property "lineCount" of "edit" is 2

    Scenario: A paragraph made after one that does not wrap does not wrap either
      When the property "html" of "root" is set to "<p style='white-space:nowrap'>x</p>"
      And the user presses "End"
      And the user presses "Return"
      And the user types "aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj kkkk llll"
      And the method "wide" of "root" is called
      Then the property "out" of "root" is "wider than the item"

    Scenario: A fenced code block of Markdown does not wrap
      When the property "markdown" of "root" is set to "```~aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj kkkk llll aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj kkkk llll~```~"
      And the method "markdownWide" of "root" is called
      Then the property "out" of "root" is "wider than the item"

    Scenario: An indented code block of Markdown wraps
      When the property "markdown" of "root" is set to "    aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj kkkk llll aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj kkkk llll~"
      And the method "markdownWide" of "root" is called
      Then the property "out" of "root" is "within the item"

  Rule: A table cell says its white space too

    A cell's own style says it, or a rule for td of a style sheet, or the
    row or the table around it. What the cell says of wrapping holds for
    its first paragraph, and a block that starts a paragraph with nothing
    before it in that paragraph joins it: if either does not wrap, the
    paragraph does not. A block after text, or after a block's end, starts
    a paragraph of its own, which wraps as that block says. Text that lands
    where the block around it does not wrap makes its paragraph not wrap.
    After a div's end, text goes on in the div's paragraph; after another
    block's end it starts one of its own.

    Scenario Outline: <case>
      When the property "html" of "root" is set to "<html>"
      And the method "plain" of "root" is called
      Then the property "out" of "root" is "<text>"

      Examples:
        | case                                             | html                                                                                                  | text      |
        | A cell collapses to a space that does not break  | <table><tr><td style='white-space:nowrap'>a   b</td></tr></table>                                     | [=a_b=]   |
        | A pre-wrap cell keeps its spaces                 | <table><tr><td style='white-space:pre-wrap'>a   b</td></tr></table>                                   | [=a   b=] |
        | So does a pre header cell                        | <table><tr><th style='white-space:pre'>a   b</th></tr></table>                                        | [=a   b=] |
        | A rule for td of a style sheet says it           | <style>td { white-space: pre-wrap }</style><table><tr><td>a   b</td></tr></table>                     | [=a   b=] |
        | A cell inherits it from its row                  | <table><tr style='white-space:pre-wrap'><td>a   b</td></tr></table>                                   | [=a   b=] |
        | A cell inherits it from its table                | <table style='white-space:pre-wrap'><tr><td>a   b</td></tr></table>                                   | [=a   b=] |
        | A paragraph in the cell may say normal           | <table><tr><td style='white-space:pre-wrap'><p style='white-space:normal'>a   b</p></td></tr></table> | [=a b=]   |
        | One right after the cell's start tag is not text | <table><tr><td style='white-space:pre-wrap'>~a</td></tr></table>                                      | [=a=]     |
        | Nor one after a list's start tag in it           | <table style='white-space:pre'><tr><td><ul>~<li>a</li></ul></td></tr></table>                         | [=a=]     |
        | Nor a numbered list's                            | <table><tr><td style='white-space:pre'><ol>~<li>a</li></ol></td></tr></table>                         | [=a=]     |
        | Nor a div's                                      | <table><tr><td style='white-space:pre-wrap'><div>~a</div></td></tr></table>                           | [=a=]     |
        | Nor a definition list's                          | <table style='white-space:pre-wrap'><tr><td><dl>~<dt>a</dt></dl></td></tr></table>                    | [=a=]     |
        | The spaces after a line break are kept           | <table><tr><td style='white-space:pre-wrap'>a<br>  b</td></tr></table>                                | [=a/  b=] |
        | After a div's end text goes on in its paragraph  | <table><tr><td><div>x</div>y</td></tr></table>                                                        | [=xy=]    |

      # Qt gives a cell paragraphs of its own; Ljós holds a cell as one, its
      # breaks line breaks (hal-c2/ljos-ui#283).
      @backlog
      Examples:
        | case                                              | html                                                                         | text      |
        | A line feed in a pre-wrap cell starts a paragraph | <table><tr><td style='white-space:pre-wrap'>a  ~b</td></tr></table>          | [=a  #b=] |
        | One after an inline start tag is                  | <table><tr><td style='white-space:pre-wrap'><b>~a</b></td></tr></table>      | [=#a=]    |
        | A div after text in a cell starts a paragraph     | <table><tr><td style='white-space:pre-wrap'>x<div>~a</div></td></tr></table> | [=x#a=]   |
        | Text after a paragraph's end is a paragraph       | <table><tr><td><p>x</p>y</td></tr></table>                                   | [=x#y=]   |

    Scenario Outline: <case>
      When the property "html" of "root" is set to "<html>"
      And the method "wide" of "root" is called
      Then the property "out" of "root" is "<width>"

      Examples:
        | case                                                  | html                                                                                                                                                                                         | width               |
        | A normal cell wraps                                   | <table><tr><td>aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj kkkk llll</td></tr></table>                                                                                                 | within the item     |
        | A nowrap cell does not                                | <table><tr><td style='white-space:nowrap'>aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj kkkk llll</td></tr></table>                                                                      | wider than the item |
        | Nor does a pre header cell                            | <table><tr><th style='white-space:pre'>aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj kkkk llll</th></tr></table>                                                                         | wider than the item |
        | A pre-wrap cell wraps                                 | <table><tr><td style='white-space:pre-wrap'>aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj kkkk llll</td></tr></table>                                                                    | within the item     |
        | A cell of a nowrap table does not                     | <table style='white-space:nowrap'><tr><td>aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj kkkk llll</td></tr></table>                                                                      | wider than the item |
        | Nor does one a rule makes nowrap                      | <style>td { white-space: nowrap }</style><table><tr><td>aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj kkkk llll</td></tr></table>                                                        | wider than the item |
        | Nor does a nowrap paragraph in a cell                 | <table><tr><td><p style='white-space:nowrap'>aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj kkkk llll</p></td></tr></table>                                                               | wider than the item |
        | A cell beside a nowrap one still wraps                | <table><tr><td style='white-space:nowrap'>x</td><td>aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj kkkk llll</td></tr></table>                                                            | within the item     |
        | A normal paragraph that starts a nowrap cell does not | <table><tr><td style='white-space:nowrap'><p style='white-space:normal'>aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj kkkk llll</p></td></tr></table>                                    | wider than the item |
        | Nor after white space that is nothing                 | <table><tr><td style='white-space:nowrap'>~<p style='white-space:normal'>aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj kkkk llll</p></td></tr></table>                                   | wider than the item |
        | Nor in a nowrap div that starts a cell                | <table><tr><td><div style='white-space:nowrap'><p style='white-space:normal'>aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj kkkk llll</p></div></td></tr></table>                         | wider than the item |
        | Nor on the line after a break in it                   | <table><tr><td style='white-space:nowrap'><p style='white-space:normal'>x<br>aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj kkkk llll</p></td></tr></table>                               | wider than the item |
        | Text after it has the cell's nowrap                   | <table><tr><td style='white-space:nowrap'><p style='white-space:normal'>x</p>aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj kkkk llll</td></tr></table>                                   | wider than the item |
        | Text after a div's end makes its paragraph not wrap   | <table><tr><td style='white-space:nowrap'>z<div style='white-space:normal'>x</div>aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj kkkk llll</td></tr></table>                              | wider than the item |
        | A paragraph after an empty one wraps                  | <table><tr><td style='white-space:nowrap'><p style='white-space:normal'></p><p style='white-space:normal'>aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj kkkk llll</p></td></tr></table>  | within the item     |
        | Text after an empty nowrap paragraph wraps            | <table><tr><td>x<p style='white-space:nowrap'></p>aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj kkkk llll</td></tr></table>                                                              | within the item     |

      # Text after a paragraph's end in a cell is a paragraph of its own in
      # Qt, and each paragraph of a cell wraps as it says. A TextEdit holds
      # a cell as one paragraph, with one wrap for all of it; a Text reads
      # them apart (below) (hal-c2/ljos-ui#283).
      @backlog
      Examples:
        | case                                                      | html                                                                                                                                                                                                            | width           |
        | Text after a nowrap paragraph wraps                       | <table><tr><td><p style='white-space:nowrap'>x</p>aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj kkkk llll</td></tr></table>                                                                                 | within the item |
        | A normal paragraph after text in it wraps                 | <table><tr><td style='white-space:nowrap'><p style='white-space:normal'>x</p><p style='white-space:normal'>aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj kkkk llll</p></td></tr></table>                    | within the item |
        | So does one after a line break                            | <table><tr><td style='white-space:nowrap'>x<br><p style='white-space:normal'>aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj kkkk llll</p></td></tr></table>                                                  | within the item |
        | A paragraph after a kept line feed in a nowrap cell wraps | <table><tr><td style='white-space:nowrap'><span style='white-space:pre-wrap'>~</span><p style='white-space:normal'>aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj kkkk llll</p></td></tr></table>            | within the item |
        | So does one after kept spaces                             | <table><tr><td style='white-space:nowrap'><span style='white-space:pre-wrap'>  </span><p style='white-space:normal'>aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj kkkk llll</p></td></tr></table>           | within the item |
        | And one after a kept line feed in a nowrap div            | <table><tr><td><div style='white-space:nowrap'><span style='white-space:pre-wrap'>~</span><p style='white-space:normal'>aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj kkkk llll</p></div></td></tr></table> | within the item |

    Scenario Outline: <case>
      When the property "html" of "root" is set to "<html>"
      And the method "wide" of "label" is called
      Then the property "out" of "root" is "<width>"

      Examples:
        | case                                                                    | html                                                                                                                                                                                                            | width               |
        | A Text keeps a normal paragraph that starts a nowrap cell from wrapping | <table><tr><td style='white-space:nowrap'><p style='white-space:normal'>aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj kkkk llll</p></td></tr></table>                                                       | wider than the item |
        | A Text wraps a normal paragraph after text in a nowrap cell             | <table><tr><td style='white-space:nowrap'><p style='white-space:normal'>x</p><p style='white-space:normal'>aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj kkkk llll</p></td></tr></table>                    | within the item     |
        | A Text wraps one after a line break                                     | <table><tr><td style='white-space:nowrap'>x<br><p style='white-space:normal'>aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj kkkk llll</p></td></tr></table>                                                  | within the item     |
        | A Text keeps text after a div's end from wrapping                       | <table><tr><td style='white-space:nowrap'>z<div style='white-space:normal'>x</div>aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj kkkk llll</td></tr></table>                                                 | wider than the item |
        | A Text wraps a paragraph after a kept line feed in a nowrap cell        | <table><tr><td style='white-space:nowrap'><span style='white-space:pre-wrap'>~</span><p style='white-space:normal'>aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj kkkk llll</p></td></tr></table>            | within the item     |
        | A Text wraps one after kept spaces                                      | <table><tr><td style='white-space:nowrap'><span style='white-space:pre-wrap'>  </span><p style='white-space:normal'>aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj kkkk llll</p></td></tr></table>           | within the item     |
        | A Text wraps one after a kept line feed in a nowrap div                 | <table><tr><td><div style='white-space:nowrap'><span style='white-space:pre-wrap'>~</span><p style='white-space:normal'>aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj kkkk llll</p></div></td></tr></table> | within the item     |

  Rule: White space between blocks goes to the block before

    White space after a block ends, before the next one has text, is at
    the end of the block before: one space where spaces collapse, unless
    that block ends in a space or a line break already, and as it was
    written where they are kept. A space that collapsed where spaces do
    not break counts as a space there; a no-break space written as one
    does not. It has the format of where it was written. One character
    of it, right after the end tag of a block that was open, is nothing
    when a tag follows it. A comment eats the white space after it where
    spaces collapse. Between the cells of a table, and after a table, it
    is nothing.

    Scenario Outline: <case>
      When the property "html" of "root" is set to "<html>"
      And the method "plain" of "root" is called
      Then the property "out" of "root" is "<text>"

      Examples:
        | case                                                     | html                                                                   | text       |
        | Two spaces between paragraphs end the first with one     | <p>a</p>  <p>b</p>                                                     | [a #b]     |
        | So do a line feed and spaces                             | <p>a</p>~  <p>b</p>                                                    | [a #b]     |
        | One space after a block's end tag is nothing             | <p>a</p> <p>b</p>                                                      | [a#b]      |
        | So is one line feed                                      | <p>a</p>~<p>b</p>                                                      | [a#b]      |
        | One line feed at the end is a space                      | <p>a</p>~                                                              | [a ]       |
        | An end tag that closes nothing is no tag                 | <p>a</p>~</b>                                                          | [a ]       |
        | After an inline tag one is a space                       | <p>a</p><b></b>~<p>b</p>                                               | [a #b]     |
        | Before an inline tag one is nothing                      | <p>a</p>~<b>b</b>                                                      | [a#b]      |
        | Not after a block that ends in a space                   | <p>a </p>  <p>b</p>                                                    | [a #b]     |
        | Nor after one that ends in a line break                  | <p>a<br></p>  <p>b</p>                                                 | [a/#b]     |
        | Nor after one that ends in a space that does not break   | <div style='white-space:nowrap'><p>a </p>  <p>b</p></div>              | [a_#b]     |
        | A no-break space written as one is not a space here      | <p>a&nbsp;</p>  <p>b</p>                                               | [a_ #b]    |
        | Nor where spaces do not break                            | <div style='white-space:nowrap'><p>a&nbsp;</p>  <p>b</p></div>         | [a__#b]    |
        | After a list it goes to its last item                    | <ul><li>a</li></ul>  <p>b</p>                                          | [a #b]     |
        | In a list, to the item before                            | <ul><li>a<ul><li>b</li></ul>  </li></ul>                               | [a#b ]     |
        | At the end of the document too                           | <p>a</p>~<p>b</p>~~                                                    | [a#b ]     |
        | Before an empty paragraph too                            | <p>a</p>  <p></p>                                                      | [a #]      |
        | Text after a block is a block of its own                 | <p>a</p>  b                                                            | [a#b]      |
        | After a block's start tag it is nothing                  | <p>x</p><div>  <p>a</p></div>                                          | [x#a]      |
        | A comment eats the white space after it                  | <p>a<!-- -->  b</p>                                                    | [ab]       |
        | White space before a comment counts for the tag after it | <p>a</p> <!-- x --><p>b</p>                                            | [a#b]      |
        | No-break spaces are kept as they are                     | <p>a</p>&nbsp;&nbsp;<p>b</p>                                           | [a__#b]    |
        | One no-break space after a block's end tag is nothing    | <p>a</p>&nbsp;<p>b</p>                                                 | [a#b]      |
        | Before a table it goes to the paragraph                  | <p>a</p>  <table><tr><td>x</td></tr></table>                           | [a =x=]    |
        | After a table it is nothing                              | <table><tr><td>x</td></tr></table>  b                                  | [=x=b]     |
        | Between cells it is nothing                              | <table><tr><td>a</td>  <td>b</td></tr>  </table>                       | [=a=b=]    |
        | pre keeps it as written                                  | <div style='white-space:pre'><p>a</p>  <p>b</p></div>                  | [a  #b]    |
        | A line feed in pre starts a paragraph                    | <div style='white-space:pre'><p>a</p>~~~<p>b</p></div>                 | [a####b]   |
        | One line feed after a block's end tag is nothing in pre  | <div style='white-space:pre'><p>a</p>~<p>b</p></div>                   | [a#b]      |
        | pre-wrap leaves out line feeds before an end tag         | <div style='white-space:pre-wrap'><p>a</p>~~</div><p>b</p>             | [a#b]      |
        | and keeps the spaces there                               | <div style='white-space:pre-wrap'><p>a</p>  </div>                     | [a  ]      |
        | After the start tag of a pre div it is a block           | <div style='white-space:pre'>  <p>a</p></div>                          | [  #a]     |
        | pre-line collapses it                                    | <div style='white-space:pre-line'><p>a</p>  <p>b</p></div>             | [a #b]     |
        | nowrap makes it a space that does not break              | <div style='white-space:nowrap'><p>a</p>  <p>b</p></div>               | [a_#b]     |

    Scenario Outline: The space between blocks has the format it was written in: <case>
      When the property "html" of "root" is set to "<html>"
      And the method "spaceBold" of "root" is called
      Then the property "out" of "root" is "<bold>"

      Examples:
        | case                         | html                                                     | bold     |
        | in a bold div                | <div style='font-weight:bold'><p>a</p>  <p>b</p></div> | bold     |
        | after bold text, not bold    | <p><b>a</b></p>  <p>b</p>                                | not bold |

    Scenario Outline: With the editor's mark: <case>
      When the property "html" of "root" is set to "<html>"
      And the method "plain" of "root" is called
      Then the property "out" of "root" is "<text>"

      Examples:
        | case                                        | html                                                                              | text      |
        | It is kept as written, without line feeds   | <meta name='qrichtext' content='1'><p>a</p>~  <p>b</p>                            | [a  #b]   |
        | One character after a block's end tag too   | <meta name='qrichtext' content='1'><p>a</p> <p>b</p>                              | [a#b]     |
        | Before an end tag too                       | <meta name='qrichtext' content='1'><p>a</p>  </body>                              | [a  ]     |
        | After the start tag of a div it is a block  | <meta name='qrichtext' content='1'><div>  <p>a</p></div>                          | [  #a]    |
        | After the start tag of a list it is nothing | <meta name='qrichtext' content='1'><ul>  <li>a</li></ul>                          | [a]       |
        | Between cells it goes to the cell before    | <meta name='qrichtext' content='1'><table><tr><td>a</td>  <td>b</td></tr></table> | [=a  =b=] |
        | After a table it goes to the block after    | <meta name='qrichtext' content='1'><table><tr><td>a</td></tr></table>  <p>b</p>   | [=a=  #b] |
        | A comment is no text and eats nothing       | <meta name='qrichtext' content='1'><p>a</p><!-- -->  <p>b</p>                     | [a  #b]   |

  Rule: The HTML a TextEdit writes reads back with its spaces

    Scenario: Spaces read from HTML are written so they read back
      When the property "html" of "root" is set to "<p style='white-space:pre-wrap'>    x  y</p>"
      And the method "again" of "root" is called
      Then the property "out" of "root" is "[    x  y]"

    Scenario: Indentation the user typed reads back
      When the user types "a"
      And the user presses "Return"
      And the user types "    b  c"
      And the method "again" of "root" is called
      Then the property "out" of "root" is "[a#    b  c]"

  Rule: HTML with the editor's mark is read as written, and wraps

    The HTML a TextEdit writes starts with the mark
    `<meta name="qrichtext" content="1">`. Where it stands, anywhere in the
    document, every space and tab of the HTML is text and no line feed is;
    white-space is not read, and every paragraph wraps. The name must come
    before the content, and the content must be 1.

    Scenario Outline: <case>
      When the property "html" of "root" is set to "<html>"
      And the method "plain" of "root" is called
      Then the property "out" of "root" is "<text>"

      Examples:
        | case                                  | html                                                                      | text      |
        | Every space is kept                   | <meta name='qrichtext' content='1'><p>  a   b</p>                         | [  a   b] |
        | A line feed is no text                | <meta name='qrichtext' content='1'><p>a~b</p>                             | [ab]      |
        | white-space is not read               | <meta name='qrichtext' content='1'><p style='white-space:nowrap'>a  b</p> | [a  b]    |
        | The mark counts after the text too    | <p>a   b</p><meta name='qrichtext' content='1'>                           | [a   b]   |
        | The name must come before the content | <meta content='1' name='qrichtext'><p>a   b</p>                           | [a b]     |
        | Another content is no mark            | <meta name='qrichtext' content='2'><p>a   b</p>                           | [a b]     |

    Scenario: A paragraph that does not wrap is written so that it reads back wrapping
      When the property "html" of "root" is set to "<p style='white-space:pre'>aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj kkkk llll</p>"
      And the method "againWide" of "root" is called
      Then the property "out" of "root" is "within the item"
