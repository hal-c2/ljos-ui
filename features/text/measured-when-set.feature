# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-text.html (contentWidth, contentHeight, lineCount, truncated)
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html (contentWidth, lineCount, paintedWidth)
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#implicitWidth-prop
# Verified on Qt 6.11.2 (qml6, offscreen).
#
# What qml6 gives a script that changes a shown item and reads a size of
# it in the next statement (Text unless named; Liberation Mono, 20 px):
#
#   change                      read                           before -> after
#   text "Hamburgefonstiv"      contentWidth, implicitWidth    0 -> 180
#   font.pixelSize 40           contentHeight, implicitHeight  23 -> 46
#   font.pixelSize 40           baselineOffset                 16.6 -> 33.3
#   width 100 (wrapMode Wrap)   lineCount, contentHeight       1, 46 -> 7, 322
#   wrapMode Wrap               lineCount                      1 -> 7
#   elide ElideRight            truncated, contentWidth        false, 648 -> true, 96
#   Label text                  contentWidth, implicitWidth    new at once
#   TextEdit text               contentWidth, paintedWidth,    new at once
#                               implicitWidth
#   TextEdit width (Wrap)       lineCount, contentWidth        new at once
#
# Text's paintedWidth and paintedHeight read as its content size there;
# Ljós's Text has neither yet. Each change signal is emitted inside the
# statement that made the change, and a later frame changes nothing.

@qt-parity @api-QtQuick.Text.contentWidth @api-QtQuick.Text.lineCount
@api-QtQuick.Text.truncated @api-QtQuick.TextEdit.contentWidth
Feature: A shown text's sizes are new in the statement after it changes
  A script that gives a shown text another text, font, width, wrap mode
  or elide mode, and then asks how large it is, gets the size the text has
  now, not the one it had before: a text is laid out when a size of it is
  read, if it changed since it was last laid out.

  Scenario Outline: <what> reads its new <size> at once
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        id: root; objectName: "root"; width: 400; height: 300
        property var before; property var after
        function change() {
          root.before = subject.<size>
          <change>
          root.after = subject.<size>
        }
        <item> { id: subject; <start> }
        <item> { id: ref; y: 100; <end> }
        property bool moved: root.before !== root.after
        property bool fresh: root.after === ref.<size>
      }
      """
    When 50 milliseconds pass
    And the method "change" of "root" is called
    Then the property "moved" of "root" is true
    And the property "fresh" of "root" is true

    Examples:
      | what                  | item     | size           | start                                          | change                           | end                                            |
      | A text                | Text     | contentWidth   | text: "hi"                                     | subject.text = "Hamburgefonstiv" | text: "Hamburgefonstiv"                        |
      | A text                | Text     | implicitWidth  | text: "hi"                                     | subject.text = "Hamburgefonstiv" | text: "Hamburgefonstiv"                        |
      | A text in a new font  | Text     | contentHeight  | text: "hi"                                     | subject.font.pixelSize = 40      | text: "hi"; font.pixelSize: 40                 |
      | A text in a new font  | Text     | implicitHeight | text: "hi"                                     | subject.font.pixelSize = 40      | text: "hi"; font.pixelSize: 40                 |
      | A text in a new font  | Text     | baselineOffset | text: "hi"                                     | subject.font.pixelSize = 40      | text: "hi"; font.pixelSize: 40                 |
      | A narrower text       | Text     | lineCount      | width: 300; wrapMode: Text.Wrap; text: "a b c d e f" | subject.width = 20         | width: 20; wrapMode: Text.Wrap; text: "a b c d e f" |
      | A narrower text       | Text     | contentHeight  | width: 300; wrapMode: Text.Wrap; text: "a b c d e f" | subject.width = 20         | width: 20; wrapMode: Text.Wrap; text: "a b c d e f" |
      | A text that wraps     | Text     | lineCount      | width: 20; text: "a b c d e f"                 | subject.wrapMode = Text.Wrap     | width: 20; wrapMode: Text.Wrap; text: "a b c d e f" |
      | An elided text        | Text     | truncated      | width: 40; text: "Hamburgefonstiv"             | subject.elide = Text.ElideRight  | width: 40; elide: Text.ElideRight; text: "Hamburgefonstiv" |
      | An elided text        | Text     | contentWidth   | width: 40; text: "Hamburgefonstiv"             | subject.elide = Text.ElideRight  | width: 40; elide: Text.ElideRight; text: "Hamburgefonstiv" |
      | A label               | Label    | contentWidth   | text: "hi"                                     | subject.text = "Hamburgefonstiv" | text: "Hamburgefonstiv"                        |
      | A label               | Label    | implicitWidth  | text: "hi"                                     | subject.text = "Hamburgefonstiv" | text: "Hamburgefonstiv"                        |
      | A text editor         | TextEdit | contentWidth   | text: "hi"                                     | subject.text = "Hamburgefonstiv" | text: "Hamburgefonstiv"                        |
      | A text editor         | TextEdit | paintedWidth   | text: "hi"                                     | subject.text = "Hamburgefonstiv" | text: "Hamburgefonstiv"                        |
      | A text editor         | TextEdit | implicitWidth  | text: "hi"                                     | subject.text = "Hamburgefonstiv" | text: "Hamburgefonstiv"                        |
      | A narrower text editor | TextEdit | lineCount     | width: 300; wrapMode: TextEdit.Wrap; text: "a b c d e f" | subject.width = 20   | width: 20; wrapMode: TextEdit.Wrap; text: "a b c d e f" |

  Scenario: A text set as its item is completed reads its new width at once
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 400; height: 300
        property real early: -1
        Text { id: t }
        Text { id: ref; y: 100; text: "Hamburgefonstiv" }
        Component.onCompleted: { t.text = "Hamburgefonstiv"; root.early = t.contentWidth }
        property bool fresh: root.early > 0 && root.early === ref.contentWidth
      }
      """
    Then the property "fresh" of "root" is true
