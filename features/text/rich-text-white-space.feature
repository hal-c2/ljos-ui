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
  The HTML a TextEdit writes says `white-space: pre-wrap` for its
  paragraphs, so its spaces read back as they were.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 300; height: 200
        property string html
        property string out
        onHtmlChanged: edit.text = html.split("~").join("\n")
        function show(e) {
          const t = e.getText(0, e.length)
          out = "[" + t.split("\u2028").join("/").split("\u2029").join("#").split("\u00a0").join("_").replace(/[\ufdd0\ufdd1]/g, "=") + "]"
        }
        function plain() { show(edit) }
        function lines() { out = edit.lineCount > 1 ? "wraps" : "one line" }
        function again() { other.text = edit.text; show(other) }
        TextEdit { id: edit; objectName: "edit"; width: 280; focus: true; wrapMode: TextEdit.WordWrap; textFormat: TextEdit.RichText }
        TextEdit { id: other; textFormat: TextEdit.RichText }
      }
      """

  Rule: pre and pre-wrap keep every space

    Scenario Outline: <case>
      When the property "html" of "root" is set to "<html>"
      And the method "plain" of "root" is called
      Then the property "out" of "root" is "<text>"

      Examples:
        | case                                               | html                                                                                              | text                  |
        | Indentation after a line break is kept             | <p style='margin:0;white-space:pre-wrap'>def f():<br/>    return 1</p>                            | [def f():/    return 1] |
        | pre keeps a run of spaces                          | <p style='white-space:pre'>a   b</p>                                                              | [a   b]               |
        | pre-wrap keeps spaces at both ends                 | <p style='white-space:pre-wrap'>  a   b  </p>                                                     | [  a   b  ]           |
        | The space before a line break is kept              | <p style='white-space:pre-wrap'>a  <br>  b</p>                                                    | [a  /  b]             |
        | A rule for p of a style sheet says it              | <style>p, li { white-space: pre-wrap; }</style><p>  a   b</p>                                     | [  a   b]             |
        | A rule for li of a style sheet says it             | <style>p, li { white-space: pre-wrap; }</style><ul><li>  a   b</li></ul>                          | [  a   b]             |
        | What is inside the paragraph inherits it           | <p style='white-space:pre-wrap'><b>  a</b>   b</p>                                                | [  a   b]             |
        | A paragraph inherits it from a div around it       | <div style='white-space:pre-wrap'><p>a   b</p></div>                                              | [a   b]               |
        | An element inside may say normal                   | <p style='white-space:pre-wrap'><span style='white-space:normal'>a   b</span>  c</p>              | [a b  c]              |
        | A span may say it for its own text                 | <span style='white-space:pre-wrap'>a   b</span>  c                                                | [a   b c]             |
        | The element's own style wins over a rule           | <style>p { white-space: pre-wrap }</style><p style='white-space:normal'>a   b</p>                 | [a b]                 |
        | A value nobody knows leaves what is inherited      | <div style='white-space:pre-wrap'><p style='white-space:bogus'>a   b</p></div>                    | [a   b]               |

  Rule: Where line feeds are kept, one starts a new paragraph

    Scenario Outline: <case>
      When the property "html" of "root" is set to "<html>"
      And the method "plain" of "root" is called
      Then the property "out" of "root" is "<text>"

      Examples:
        | case                                                     | html                                              | text     |
        | A line feed in pre-wrap starts a paragraph               | <p style='white-space:pre-wrap'>a  ~b</p>         | [a  #b]  |
        | An empty line is an empty paragraph                      | <p style='white-space:pre-wrap'>a~~b</p>          | [a##b]   |
        | A line feed right after the start tag is not text        | <p style='white-space:pre-wrap'>~~  a</p>         | [#  a]   |
        | A line feed at the end makes no paragraph                | <p style='white-space:pre-wrap'>a~</p>            | [a]      |
        | So it is in pre                                          | <pre>~  a</pre>                                   | [  a]    |
        | So it is in a table cell                                 | <table><tr><td><pre>~code</pre></td></tr></table> | [=code=]           |
        | pre-line keeps the line feed, not the spaces around it   | <p style='white-space:pre-line'>  a  ~  b</p>     | [a#b]    |

  Rule: normal, nowrap and pre-line collapse spaces

    Scenario Outline: <case>
      When the property "html" of "root" is set to "<html>"
      And the method "plain" of "root" is called
      Then the property "out" of "root" is "<text>"

      Examples:
        | case                                  | html                                          | text   |
        | Without white-space spaces collapse   | <p>  a   b</p>                                | [a b]  |
        | normal collapses                      | <p style='white-space:normal'>  a   b</p>     | [a b]  |
        | nowrap collapses to one that does not break | <p style='white-space:nowrap'>  a   b</p>     | [a_b]  |
        | pre-line collapses                    | <p style='white-space:pre-line'>  a   b</p>   | [a b]  |
        | normal on a pre collapses             | <pre style='white-space:normal'>a   b</pre>   | [a b]  |

  Rule: nowrap does not wrap at its spaces

    Its spaces do not break, so a line wrapped at words stays whole.

    Scenario Outline: <case>
      When the property "html" of "root" is set to "<html>"
      And the method "lines" of "root" is called
      Then the property "out" of "root" is "<lines>"

      Examples:
        | case                                | html                                                                                                 | lines    |
        | A long normal paragraph wraps       | <p style='white-space:normal'>aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj kkkk llll</p>        | wraps    |
        | A long nowrap paragraph stays whole | <p style='white-space:nowrap'>aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj kkkk llll</p>        | one line |

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
