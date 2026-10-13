# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#isRightToLeft-method
#   https://doc.qt.io/qt-6/qkeysequence.html#standard-shortcuts
# Verified on Qt 6.11.2 (qml6, offscreen; key input over its vnc platform).

@qt-parity
Feature: Editing text that mixes right-to-left and left-to-right
  Unlike a TextInput, a TextEdit moves through the text in the order it is
  written: Right goes to the next character in a paragraph that reads left
  to right, also through a stretch of Hebrew, and Left goes to the next one
  in a paragraph that reads right to left, also through a stretch of
  English. Plain text and rich text move alike. Home and End go to the
  logical start and end of the line.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 320; height: 200
        property string out
        function direction(name, start, end) {
          const edit = name === "plain" ? plain : rich
          out = edit.isRightToLeft(start, end) ? "rtl" : "ltr"
        }
        TextEdit {
          id: plain; objectName: "plain"; width: 300; height: 40
          text: "ab שלום cd"
        }
        TextEdit {
          id: rich; objectName: "rich"; y: 50; width: 300; height: 40
          textFormat: TextEdit.RichText
          text: "ab <b>של</b>ום cd"
        }
        TextEdit {
          id: hebrew; objectName: "hebrew"; y: 100; width: 300; height: 40
          textFormat: TextEdit.RichText
          text: "שלום <i>ab</i> של"
        }
      }
      """

  Scenario Outline: <key> from <from> in the <edit> text with Hebrew inside
    When the property "focus" of "<edit>" is set to true
    And the property "cursorPosition" of "<edit>" is set to <from>
    And the user presses "<key>"
    Then the property "cursorPosition" of "<edit>" is <to>

    Examples:
      | edit  | key   | from | to |
      | plain | Right | 1    | 2  |
      | plain | Right | 3    | 4  |
      | plain | Right | 5    | 6  |
      | plain | Left  | 5    | 4  |
      | plain | Right | 7    | 8  |
      | plain | Left  | 8    | 7  |
      | plain | Home  | 5    | 0  |
      | plain | End   | 5    | 10 |
      | rich  | Right | 1    | 2  |
      | rich  | Right | 3    | 4  |
      | rich  | Right | 5    | 6  |
      | rich  | Left  | 5    | 4  |
      | rich  | Right | 7    | 8  |
      | rich  | Left  | 8    | 7  |
      | rich  | Home  | 5    | 0  |
      | rich  | End   | 5    | 10 |

  Scenario Outline: <key> from <from> in a Hebrew text with English inside
    When the property "focus" of "hebrew" is set to true
    And the property "cursorPosition" of "hebrew" is set to <from>
    And the user presses "<key>"
    Then the property "cursorPosition" of "hebrew" is <to>

    Examples:
      | key   | from | to |
      | Left  | 0    | 1  |
      | Right | 1    | 0  |
      | Left  | 4    | 5  |
      | Left  | 5    | 6  |
      | Right | 6    | 5  |
      | Home  | 6    | 0  |
      | End   | 6    | 10 |

  Scenario Outline: Shift+Right from <from> selects through the <edit> text
    When the property "focus" of "<edit>" is set to true
    And the property "cursorPosition" of "<edit>" is set to <from>
    And the user presses "Shift+Right"
    And the user presses "Shift+Right"
    Then the property "selectionStart" of "<edit>" is <start>
    And the property "selectionEnd" of "<edit>" is <end>

    Examples:
      | edit  | from | start | end |
      | plain | 1    | 1     | 3   |
      | plain | 4    | 4     | 6   |
      | rich  | 1    | 1     | 3   |
      | rich  | 4    | 4     | 6   |

  Scenario Outline: The <edit> text reads right to left only where it is Hebrew
    When the method "direction" of "root" is called with "<edit>", <start>, <end>
    Then the property "out" of "root" is "<direction>"

    Examples:
      | edit  | start | end | direction |
      | plain | 0     | 2   | ltr       |
      | plain | 3     | 7   | rtl       |
      | rich  | 3     | 7   | rtl       |
      | rich  | 8     | 10  | ltr       |
