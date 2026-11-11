# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#textFormat-prop
# Verified on Qt 6.12.0 (qml6, offscreen).

@qt-parity
Feature: Changing the format of a TextEdit
  Setting textFormat on a TextEdit that holds text converts what it holds,
  as Qt 6.12 does. A rich document stays as it is under RichText,
  MarkdownText or AutoText, and its text is written in the new format;
  under PlainText its characters become the text as the old format wrote
  it. Plain characters are read again in the new format, and under
  AutoText as AutoText reads them. The cursor goes to the start unless the
  document keeps the format it had. Line feeds read as tildes; the
  characters are in brackets, with # between paragraphs, and of a text of
  HTML only its body is shown.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 300; height: 200
        property string out
        function simple(h) {
          return h.replace(/^[\s\S]*<body[^>]*>\n?/, "").replace(/<\/body>[\s\S]*$/, "")
            .replace(/ ?margin-top:0px; margin-bottom:0px; margin-left:0px; margin-right:0px;( -qt-block-indent:0; text-indent:0px;)?/g, "")
            .replace(/ style=""/g, "").split("\"").join("'").split("\n").join("~")
        }
        function shown(t) { return t.indexOf("<body") >= 0 ? simple(t) : t.split("\n").join("~") }
        function start(format, text) {
          edit.textFormat = TextEdit[format]
          edit.text = text.split("[").join("<").split("]").join(">").split("~").join("\n")
          edit.cursorPosition = 1
        }
        function change(format) {
          edit.textFormat = TextEdit[format]
          let chars = edit.getText(0, edit.length)
          chars = chars.indexOf("<body") >= 0 ? "html:" + simple(chars) : chars.split("\u2029").join("#").split("\n").join("~")
          out = edit.cursorPosition + " " + shown(edit.text) + " [" + chars + "]"
        }
        TextEdit { id: edit; objectName: "edit"; width: 280 }
      }
      """

  Scenario Outline: <from> of "<text>" made <to> reads "<out>"
    When the method "start" of "root" is called with "<from>", "<text>"
    And the method "change" of "root" is called with "<to>"
    Then the property "out" of "root" is "<out>"

    Examples:
      | from         | text            | to           | out                                                                                                                                                                                              |
      | MarkdownText | a **b** c       | RichText     | 0 <p style=' margin-top:6px; margin-bottom:6px; margin-left:0px; margin-right:0px; -qt-block-indent:0; text-indent:0px;'>a <span style=' font-weight:700;'>b</span> c</p> [a b c]                |
      | MarkdownText | a~~```~x y~```~ | PlainText    | 0 a~~```~x y~```~~ [a~~```~x y~```~~]                                                                                                                                                            |
      | MarkdownText | a **b** c       | PlainText    | 0 a **b** c~~ [a **b** c~~]                                                                                                                                                                      |
      | MarkdownText | a **b** c       | AutoText     | 1 a **b** c~~ [a b c]                                                                                                                                                                            |
      | RichText     | [b]a[/b] b      | MarkdownText | 0 **a** b~~ [a b]                                                                                                                                                                                |
      | RichText     | [b]a[/b] b      | PlainText    | 0 <p><span style=' font-weight:700;'>a</span> b</p> [html:<p><span style=' font-weight:700;'>a</span> b</p>]                                                                                     |
      | RichText     | [b]a[/b] b      | AutoText     | 1 <p><span style=' font-weight:700;'>a</span> b</p> [a b]                                                                                                                                        |
      | PlainText    | [b]a[/b] b      | RichText     | 0 <p><span style=' font-weight:700;'>a</span> b</p> [a b]                                                                                                                                        |
      | PlainText    | **a** b         | MarkdownText | 0 **a** b~~ [a b]                                                                                                                                                                                |
      | PlainText    | [b]a[/b] b      | AutoText     | 0 <p><span style=' font-weight:700;'>a</span> b</p> [a b]                                                                                                                                        |
      | AutoText     | [b]a[/b] b      | RichText     | 1 <p><span style=' font-weight:700;'>a</span> b</p> [a b]                                                                                                                                        |
      | AutoText     | [b]a[/b] b      | MarkdownText | 0 **a** b~~ [a b]                                                                                                                                                                                |
      | AutoText     | [b]a[/b] b      | PlainText    | 0 <p><span style=' font-weight:700;'>a</span> b</p> [html:<p><span style=' font-weight:700;'>a</span> b</p>]                                                                                     |
      | AutoText     | **a** b         | MarkdownText | 0 **a** b~~ [a b]                                                                                                                                                                                |
      | AutoText     | **a** b         | RichText     | 0 <p>**a** b</p> [**a** b]                                                                                                                                                                       |

    # Qt 6.12 writes a fenced code block as a paragraph in a monospace
    # font, which reads back as a paragraph; Ljós writes it as a pre that
    # says it does not wrap, so that it reads back as the code block it is.
    @qt-diverges
    Examples:
      | from         | text            | to       | out |
      | MarkdownText | a~~```~x y~```~ | RichText | 0 <p style=' margin-top:6px; margin-bottom:6px; margin-left:0px; margin-right:0px; -qt-block-indent:0; text-indent:0px;'>a</p>~<pre style=' white-space:pre;'><span style=' font-family:'monospace';'>x y</span></pre> [a#x y] |
