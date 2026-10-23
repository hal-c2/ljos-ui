# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#cursorSelection-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: The format cursorSelection reads in a rich TextEdit
  cursorSelection reads as the format at the cursor, whichever end of the
  selection the cursor is at: that of the character before it, or at the
  start of a paragraph that of the character after it. The text is "Bo"
  in bold, "ld ", "it" in italic and "al", then a paragraph of "X" in
  bold and "y".

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 400; height: 300
        property string out
        function shown() { const s = edit.cursorSelection; out = s.font.bold + "," + s.font.italic }
        function pick(a, b) { edit.select(a, b); shown() }
        function at(a) { edit.cursorPosition = a; shown() }
        TextEdit {
          id: edit; objectName: "edit"; width: 400
          textFormat: TextEdit.RichText; font.family: "Liberation Sans"; font.pixelSize: 20
          text: "<p><b>Bo</b>ld <i>it</i>al</p><p><b>X</b>y</p>"
        }
      }
      """

  Rule: The format is the cursor's

    Scenario Outline: select(<a>, <b>) reads as bold <bold> and italic <italic>
      When the method "pick" of "root" is called with <a>, <b>
      Then the property "out" of "root" is "<bold>,<italic>"

      Examples:
        | a  | b  | bold  | italic |
        | 1  | 4  | false | false  |
        | 4  | 1  | true  | false  |
        | 4  | 6  | false | true   |
        | 8  | 10 | true  | false  |
        | 9  | 11 | true  | false  |
        | 11 | 9  | false | false  |
        | 10 | 0  | true  | false  |

    Scenario Outline: The cursor at <a> reads as bold <bold>
      When the method "at" of "root" is called with <a>
      Then the property "out" of "root" is "<bold>,false"

      Examples:
        | a  | bold  |
        | 10 | true  |
        | 9  | false |
