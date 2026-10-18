# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#selectByMouse-prop
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#mouseSelectionMode-prop
#   https://doc.qt.io/qt-6/qml-qtquick-textinput.html#mouseSelectionMode-prop
#   https://doc.qt.io/qt-6/qml-qtquick-controls-textarea.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Where a drag that selects ends
  A drag past the end of a line selects to that end, without the line
  break; below the last line it selects to the end of the text and above
  the first to its start. A drag that began with a double click selects
  whole words: in a TextEdit it takes in the word under the pointer and
  keeps what it had while the pointer is over a space or past a line's
  end, and below the last line it reads that line. In a TextInput it
  selects from the start of the word by characters. A TextArea and a
  TextField select only once the pointer has gone more than ten pixels
  sideways. The numbers are those of the bundled font, Liberation Sans,
  at 14 pixels: a line is 16 pixels.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        width: 400; height: 300
        TextEdit { objectName: "edit"; y: 20; width: 300; selectByMouse: true; font.family: "Liberation Sans"; font.pixelSize: 14; text: "row first line\nrow second line\nrow third" }
        TextArea { objectName: "area"; y: 100; width: 300; padding: 0; font.family: "Liberation Sans"; font.pixelSize: 14; text: "row first line\nrow second line\nrow third" }
        TextInput { objectName: "input"; y: 200; width: 300; selectByMouse: true; font.family: "Liberation Sans"; font.pixelSize: 14; text: "row second line" }
        TextField { objectName: "field"; y: 250; width: 300; padding: 0; font.family: "Liberation Sans"; font.pixelSize: 14; text: "row second line" }
      }
      """

  Rule: A drag from a press selects by characters

    Scenario: A drag past the end of a line selects to its end
      When the user presses the pointer at 50, 44
      And the user moves the pointer to 200, 44
      And the user releases the pointer at 200, 44
      Then the property "selectedText" of "edit" is "ond line"

    Scenario: A drag below the last line selects to the end of the text
      When the user presses the pointer at 50, 44
      And the user moves the pointer to 50, 90
      And the user releases the pointer at 50, 90
      Then the property "selectedText" of "edit" is "ond line\nrow third"

    Scenario: A drag above the first line selects to the start of the text
      When the user presses the pointer at 50, 44
      And the user moves the pointer to 50, 5
      And the user releases the pointer at 50, 5
      Then the property "selectedText" of "edit" is "row first line\nrow sec"

  Rule: A TextArea and a TextField select once the drag has gone sideways

    Scenario: A drag straight down a TextArea selects nothing
      When the user presses the pointer at 50, 124
      And the user moves the pointer to 50, 140
      And the user moves the pointer to 50, 180
      And the user releases the pointer at 50, 180
      Then the property "selectedText" of "area" is ""
      And the property "cursorPosition" of "area" is 22

    Scenario: Ten pixels sideways is not yet a drag
      When the user presses the pointer at 50, 124
      And the user moves the pointer to 60, 124
      And the user releases the pointer at 60, 124
      Then the property "selectedText" of "area" is ""

    Scenario: Past ten pixels sideways the TextArea selects, and goes on below
      When the user presses the pointer at 50, 124
      And the user moves the pointer to 61, 124
      Then the property "selectedText" of "area" is "o"
      When the user moves the pointer to 50, 180
      And the user releases the pointer at 50, 180
      Then the property "selectedText" of "area" is "ond line\nrow third"

    Scenario: A TextField waits for the same ten pixels
      When the user presses the pointer at 50, 258
      And the user moves the pointer to 60, 258
      Then the property "selectedText" of "field" is ""
      When the user moves the pointer to 62, 258
      And the user releases the pointer at 62, 258
      Then the property "selectedText" of "field" is "o"

  Rule: A drag from a double click in a TextEdit selects the words under the pointer

    Scenario Outline: A drag from "second" to <x>, <y> selects "<text>"
      When the user presses the pointer at 50, 44
      And the user releases the pointer at 50, 44
      And the user presses the pointer at 50, 44
      And the user moves the pointer to <x>, <y>
      And the user releases the pointer at <x>, <y>
      Then the property "selectedText" of "edit" is "<text>"

      Examples:
        | x   | y  | text                    |
        | 76  | 44 | second line             |
        | 74  | 44 | second                  |
        | 200 | 44 | second                  |
        | 22  | 44 | row second              |
        | 24  | 44 | second                  |
        | 200 | 90 | second                  |
        | 50  | 5  | second                  |

    Scenario: Over a word of the line above the drag selects back to it
      When the user presses the pointer at 50, 44
      And the user releases the pointer at 50, 44
      And the user presses the pointer at 50, 44
      And the user moves the pointer to 40, 28
      And the user releases the pointer at 40, 28
      Then the property "selectedText" of "edit" is "first line\nrow second"

    Scenario: Below the last line the drag reads the last line
      When the user presses the pointer at 50, 44
      And the user releases the pointer at 50, 44
      And the user presses the pointer at 50, 44
      And the user moves the pointer to 50, 90
      And the user releases the pointer at 50, 90
      Then the property "selectedText" of "edit" is "second line\nrow third"

    Scenario: Over a space or past the end the selection stays as it was
      When the user presses the pointer at 50, 44
      And the user releases the pointer at 50, 44
      And the user presses the pointer at 50, 44
      And the user moves the pointer to 80, 44
      And the user moves the pointer to 74, 44
      And the user moves the pointer to 250, 44
      And the user releases the pointer at 250, 44
      Then the property "selectedText" of "edit" is "second line"

    Scenario: Back over the word it began on the drag selects that word
      When the user presses the pointer at 50, 44
      And the user releases the pointer at 50, 44
      And the user presses the pointer at 50, 44
      And the user moves the pointer to 80, 44
      And the user moves the pointer to 40, 44
      And the user releases the pointer at 40, 44
      Then the property "selectedText" of "edit" is "second"

    Scenario: A TextArea selects words the same way once the drag has gone sideways
      When the user presses the pointer at 50, 124
      And the user releases the pointer at 50, 124
      And the user presses the pointer at 50, 124
      And the user moves the pointer to 80, 124
      Then the property "selectedText" of "area" is "second line"
      When the user moves the pointer to 50, 180
      And the user releases the pointer at 50, 180
      Then the property "selectedText" of "area" is "second line\nrow third"

  Rule: A drag from a double click in a TextInput selects by characters from the word's start

    Scenario Outline: A drag from "second" to <x> selects "<text>"
      When the user presses the pointer at 50, 208
      And the user releases the pointer at 50, 208
      And the user presses the pointer at 50, 208
      And the user moves the pointer to <x>, 208
      And the user releases the pointer at <x>, 208
      Then the property "selectedText" of "input" is "<text>"

      Examples:
        | x   | text        |
        | 40  | se          |
        | 80  | second l    |
        | 200 | second line |

    Scenario: A drag from "second" back over the space before it selects from the word's start
      When the user presses the pointer at 50, 208
      And the user releases the pointer at 50, 208
      And the user presses the pointer at 50, 208
      And the user moves the pointer to 10, 208
      And the user releases the pointer at 10, 208
      Then the property "selectedText" of "input" is "w "

  Rule: In the word selection mode a TextEdit takes in a word from its edge

    Scenario Outline: A drag from "second" to <x> selects "<text>"
      When the property "mouseSelectionMode" of "edit" is set to 1
      And the user presses the pointer at 50, 44
      And the user moves the pointer to <x>, 44
      And the user releases the pointer at <x>, 44
      Then the property "selectedText" of "edit" is "<text>"

      Examples:
        | x  | text        |
        | 73 | second      |
        | 74 | second line |
        | 24 | row second  |
        | 26 | second      |
