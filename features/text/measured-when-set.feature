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
#
# Logged around `t.text = "Hamburgefonstiv"` on a shown Text with a
# handler on each size and a Rectangle with `x: t.contentWidth`:
#
#   set | w 122.8 | iw 122.8 | cw 122.8 | x 122.8 | text | after x=122.8
#
# The same text again logs nothing; `font.pixelSize = 40` logs each size
# that moved once (w, iw, ih, bo, cw, ch) before the next statement; a
# wrapped Text made narrower logs lineCount 5 and a Rectangle with
# `height: l.implicitHeight; y: l.lineCount` reads 95 and 5 at once. A
# frame later nothing more is logged. A text's sizes change before its
# `textChanged` there; that order is not asked for here. Under the
# runner's font, a new text tells implicitWidth twice and implicitHeight
# once, though the height comes out the same, so only the content sizes
# are counted.

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

  Scenario Outline: A binding on another item follows a text's <size> in the statement after it changes
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        id: root; objectName: "root"; width: 400; height: 300
        property var before; property var after
        function change() {
          root.before = other.x
          <change>
          root.after = other.x
        }
        <item> { id: subject; <start> }
        Rectangle { id: other; x: subject.<size> }
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
      | item  | size           | start                                                | change                           | end                                                 |
      | Text  | contentWidth   | text: "hi"                                           | subject.text = "Hamburgefonstiv" | text: "Hamburgefonstiv"                             |
      | Text  | implicitWidth  | text: "hi"                                           | subject.text = "Hamburgefonstiv" | text: "Hamburgefonstiv"                             |
      | Text  | implicitHeight | text: "hi"                                           | subject.font.pixelSize = 40      | text: "hi"; font.pixelSize: 40                      |
      | Text  | lineCount      | width: 300; wrapMode: Text.Wrap; text: "a b c d e f" | subject.width = 20               | width: 20; wrapMode: Text.Wrap; text: "a b c d e f" |
      | Label | contentWidth   | text: "hi"                                           | subject.text = "Hamburgefonstiv" | text: "Hamburgefonstiv"                             |

  Scenario: A handler of a text's size runs inside the statement that changes it, once
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 400; height: 300
        property int cw: 0; property int ch: 0
        property string log: ""
        function told() { return root.cw + " " + root.ch }
        function change() {
          // What the start-up layout told is not counted.
          root.cw = 0; root.ch = 0
          root.log = "set " + told()
          t.text = "Hamburgefonstiv"
          root.log += ", text " + told()
          t.text = "Hamburgefonstiv"
          root.log += ", again " + told()
          t.font.pixelSize = 40
          root.log += ", font " + told()
        }
        Text {
          id: t; text: "hi"
          onContentWidthChanged: root.cw++
          onContentHeightChanged: root.ch++
        }
      }
      """
    When 50 milliseconds pass
    And the method "change" of "root" is called
    Then the property "log" of "root" is "set 0 0, text 1 0, again 1 0, font 2 1"
    When 50 milliseconds pass
    Then the property "cw" of "root" is 2
    And the property "ch" of "root" is 1

  Scenario: A handler of a wrapped text's line count runs inside the statement that narrows it
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 400; height: 300
        property string log: ""
        property int told: 0
        function change() {
          root.told = 0
          root.log = "set " + root.told
          t.width = 20
          root.log += ", narrower " + root.told
        }
        Text {
          id: t; width: 300; wrapMode: Text.Wrap; text: "a b c d e f"
          onLineCountChanged: root.told++
        }
      }
      """
    When 50 milliseconds pass
    And the method "change" of "root" is called
    Then the property "log" of "root" is "set 0, narrower 1"
    When 50 milliseconds pass
    Then the property "told" of "root" is 1

  Scenario: A binding on a text's line count follows a change after a read between two changes
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 400; height: 300
        property int follow: subject.lineCount
        property int seen: 0
        function change() {
          subject.text = "a\nb"
          // Read between two changes that leave the line count the same.
          root.seen = subject.lineCount
          subject.text = "c\nd"
        }
        Text { id: subject; text: "a" }
      }
      """
    When 50 milliseconds pass
    And the method "change" of "root" is called
    Then the property "seen" of "root" is 2
    And the property "follow" of "root" is 2
    When 50 milliseconds pass
    Then the property "follow" of "root" is 2

  Scenario: A handler of a text's size that changes the text again has the last word
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 400; height: 300
        property string log: ""
        function said() { return t.text + " " + (t.contentWidth === ref.contentWidth) }
        function change() {
          t.text = "Hamburgefonstiv"
          root.log = said()
          // The same text as the one the handler replaced.
          t.text = "Hamburgefonstiv"
          root.log += ", again " + said()
        }
        Text { id: t; text: "hi"; onContentWidthChanged: if (text !== "b") text = "b" }
        Text { id: ref; y: 100; text: "b" }
        property bool fresh: t.text === "b" && t.contentWidth === ref.contentWidth
      }
      """
    When 50 milliseconds pass
    And the method "change" of "root" is called
    Then the property "log" of "root" is "b true, again b true"
    When 50 milliseconds pass
    Then the property "fresh" of "root" is true

  # Its width and height are set, so nothing follows its sizes until the
  # handler is connected: the first change waits for a read or the frame,
  # and the second finds it still to be made.
  Scenario: A handler connected between two changes of a text runs once for the second
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 400; height: 300
        property int told: 0
        property int inside: -1
        function bump() { root.told++ }
        function change() {
          subject.text = "a\nb"
          subject.lineCountChanged.connect(root.bump)
          subject.text = "a\nb\nc"
          root.inside = root.told
        }
        Text { id: subject; width: 200; height: 100; text: "a" }
      }
      """
    When 50 milliseconds pass
    And the method "change" of "root" is called
    Then the property "inside" of "root" is 1
    When 50 milliseconds pass
    Then the property "told" of "root" is 1

  # Ljós lays out a text nothing follows by the frame, and that frame tells
  # a handler connected since. qml6 laid it out in the setter, before the
  # handler was there. Telling only what followed a size before the change
  # would need the change to know them; main does the same.
  @backlog
  Scenario: A handler connected after a text changes is not told of that change
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 400; height: 300
        property int told: 0
        function bump() { root.told++ }
        function change() {
          subject.text = "a\nb"
          subject.lineCountChanged.connect(root.bump)
        }
        Text { id: subject; width: 200; height: 100; text: "a" }
      }
      """
    When 50 milliseconds pass
    And the method "change" of "root" is called
    And 50 milliseconds pass
    Then the property "told" of "root" is 0
