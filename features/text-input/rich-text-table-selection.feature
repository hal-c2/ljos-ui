# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#select-method
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#selectedText-prop
#   https://doc.qt.io/qt-6/qtextcursor.html#selectedTableCells
# Verified on Qt 6.11.2 (qml6, offscreen and vnc).

@qt-parity
Feature: Selecting the cells of a table in a rich TextEdit
  A selection from one cell into another selects whole cells: every cell
  of the rectangle the two cells span. Its ends move to the starts of
  their cells, or the anchor to the end of its cell when the cursor is
  before it. The selected text is the cells' text one after another, and
  what is copied has a line break before each cell and after the last.
  Typed text or Backspace empties every selected cell, and what is typed
  goes in the cursor's cell. A selection from a cell to outside the
  table takes in the whole table. Shift with Left or Right goes from cell
  to cell along a row and no further. The table is "x", then a row of
  "pq" and "r" and a row of "s" and "tu", then "y".

  Background:
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        id: root; objectName: "root"; width: 300; height: 200; color: "white"
        property string out
        function hex(s) {
          return Array.from(s, c => c.charCodeAt(0) > 127 || c.charCodeAt(0) < 32 ? c.charCodeAt(0).toString(16) : c).join(" ")
        }
        function shown() { out = (edit.selectionStart + "-" + edit.selectionEnd + " c" + edit.cursorPosition + " " + hex(edit.selectedText)).trim() }
        function all() { out = edit.length + " c" + edit.cursorPosition + " " + hex(edit.getText(0, edit.length)) }
        function pick(a, b) { edit.forceActiveFocus(); edit.select(a, b); shown() }
        function at(i) { edit.forceActiveFocus(); edit.cursorPosition = i }
        function copied(a, b) { edit.select(a, b); edit.copy(); src.text = ""; src.paste(); out = hex(src.text) }
        TextEdit { id: src; y: 150 }
        TextEdit {
          id: edit; objectName: "edit"
          textFormat: TextEdit.RichText; font.family: "Liberation Sans"; font.pixelSize: 20
          selectionColor: "#0000ff"; selectedTextColor: "#ffffff"
          text: "x<table border='1'><tr><td>pq</td><td>r</td></tr><tr><td>s</td><td>tu</td></tr></table>y"
        }
      }
      """

  Rule: A selection across cells selects whole cells

    Scenario Outline: select(<a>, <b>) selects <shown>
      When the method "pick" of "root" is called with <a>, <b>
      Then the property "out" of "root" is "<shown>"

      Examples:
        | a  | b  | shown |
        | 2  | 4  | 2-4 c4 p q |
        | 3  | 5  | 2-5 c5 p q r |
        | 3  | 6  | 2-5 c5 p q r |
        | 3  | 9  | 2-9 c9 p q r s t u |
        | 3  | 10 | 2-9 c9 p q r s t u |
        | 9  | 3  | 2-11 c2 p q r s t u |
        | 5  | 8  | 5-7 c7 p q r s t u |
        | 8  | 5  | 5-8 c5 p q r s t u |
        | 6  | 9  | 5-9 c9 r t u |
        | 2  | 7  | 2-7 c7 p q s |
        | 0  | 5  | 0-12 c12 x fdd0 p q fdd0 r fdd0 s fdd0 t u fdd1 |
        | 5  | 13 | 1-13 c13 fdd0 p q fdd0 r fdd0 s fdd0 t u fdd1 y |

    Scenario Outline: A drag from <from> to <to> selects <shown>
      When the method "at" of "root" is called with 0
      And the user drags from <from> to <to>
      And the method "shown" of "root" is called
      Then the property "out" of "root" is "<shown>"

      Examples:
        | from   | to     | shown |
        | 6, 38  | 15, 38 | 2-3 c3 p |
        | 10, 38 | 35, 38 | 2-5 c5 p q r |
        | 10, 38 | 40, 65 | 2-9 c9 p q r s t u |
        | 35, 38 | 8, 65  | 5-7 c7 p q r s t u |
        | 10, 38 | 12, 65 | 2-7 c7 p q s |
        | 10, 38 | 5, 95  | 1-12 c12 fdd0 p q fdd0 r fdd0 s fdd0 t u fdd1 |
        | 10, 38 | 5, 10  | 0-12 c0 x fdd0 p q fdd0 r fdd0 s fdd0 t u fdd1 |
        | 5, 10  | 40, 65 | 0-12 c12 x fdd0 p q fdd0 r fdd0 s fdd0 t u fdd1 |

    Scenario Outline: Copying select(<a>, <b>) copies <text>
      When the method "copied" of "root" is called with <a>, <b>
      Then the property "out" of "root" is "<text>"

      Examples:
        | a | b | text |
        | 5 | 8 | a p q a r a s a t u a |
        | 6 | 9 | a r a t u a |
        | 3 | 6 | a p q a r a |
        | 0 | 5 | x a p q a r a s a t u a |

    Scenario Outline: With <a> to <b> selected the pixel at <x>, <y> is <colour>
      When the method "pick" of "root" is called with <a>, <b>
      Then the pixel at <x>, <y> of the window is within 8 of "<colour>"

      Examples:
        | a | b | x  | y  | colour  |
        | 3 | 9 | 6  | 30 | #0000ff |
        | 3 | 9 | 33 | 30 | #0000ff |
        | 3 | 9 | 10 | 57 | #0000ff |
        | 3 | 9 | 45 | 57 | #ffffff |
        | 5 | 8 | 6  | 30 | #ffffff |
        | 5 | 8 | 33 | 30 | #0000ff |
        | 5 | 8 | 10 | 57 | #ffffff |

  Rule: Typing over cells empties them

    Scenario Outline: <key> with <a> to <b> selected leaves <text>
      When the method "pick" of "root" is called with <a>, <b>
      And the user presses "<key>"
      And the method "all" of "root" is called
      Then the property "out" of "root" is "<text>"

      Examples:
        | a | b | key       | text |
        | 3 | 9 | Z         | 8 c6 x fdd0 fdd0 fdd0 fdd0 z fdd1 y |
        | 9 | 3 | Z         | 8 c3 x fdd0 z fdd0 fdd0 fdd0 fdd1 y |
        | 5 | 8 | Z         | 8 c5 x fdd0 fdd0 fdd0 z fdd0 fdd1 y |
        | 8 | 5 | Z         | 8 c4 x fdd0 fdd0 z fdd0 fdd0 fdd1 y |
        | 9 | 3 | Backspace | 7 c2 x fdd0 fdd0 fdd0 fdd0 fdd1 y |
        | 9 | 3 | Delete    | 7 c2 x fdd0 fdd0 fdd0 fdd0 fdd1 y |
        | 0 | 5 | Z         | 2 c1 z y |
        | 3 | 9 | Right     | 13 c9 x fdd0 p q fdd0 r fdd0 s fdd0 t u fdd1 y |
        | 3 | 9 | Left      | 13 c2 x fdd0 p q fdd0 r fdd0 s fdd0 t u fdd1 y |
        | 9 | 3 | Right     | 13 c11 x fdd0 p q fdd0 r fdd0 s fdd0 t u fdd1 y |

  Rule: Shift and the arrow keys go from cell to cell

    Scenario Outline: <k1> then <k2> after select(<a>, <b>) selects <shown>
      When the method "pick" of "root" is called with <a>, <b>
      And the user presses "<k1>"
      And the user presses "<k2>"
      And the method "shown" of "root" is called
      Then the property "out" of "root" is "<shown>"

      Examples:
        | a  | b  | k1          | k2          | shown |
        | 9  | 3  | Shift+Right | Shift+Right | 5-11 c5 r t u |
        | 9  | 3  | Shift+Left  | Shift+Left  | 2-11 c2 p q r s t u |
        | 3  | 9  | Shift+Right | Shift+Right | 2-9 c9 p q r s t u |
        | 3  | 10 | Shift+Left  | Shift+Left  | 2-7 c7 p q s |
        | 8  | 3  | Shift+Right | Shift+Right | 5-8 c5 p q r s t u |
        | 10 | 5  | Shift+Left  | Shift+Left  | 2-11 c2 p q r s t u |
        | 2  | 5  | Shift+Left  | Shift+Left  | 2-3 c3 p |
        | 3  | 6  | Shift+Left  | Shift+Left  | 3-3 c3 |
        | 3  | 9  | Shift+Up    | Shift+Up    | 0-12 c0 x fdd0 p q fdd0 r fdd0 s fdd0 t u fdd1 |
        | 9  | 3  | Shift+Down  | Shift+Down  | 1-12 c12 fdd0 p q fdd0 r fdd0 s fdd0 t u fdd1 |
        | 3  | 9  | Shift+Home  | Shift+End   | 2-9 c9 p q r s t u |

    Scenario: Shift+Right from inside a cell crosses into the next cell and no further
      When the method "at" of "root" is called with 3
      And the user presses "Shift+Right"
      And the user presses "Shift+Right"
      And the user presses "Shift+Right"
      And the method "shown" of "root" is called
      Then the property "out" of "root" is "2-5 c5 p q r"

    Scenario: Shift+Left back into the cell the selection started in selects in that cell
      When the method "at" of "root" is called with 3
      And the user presses "Shift+Right"
      And the user presses "Shift+Right"
      And the user presses "Shift+Left"
      And the method "shown" of "root" is called
      Then the property "out" of "root" is "3-4 c4 q"

    Scenario: Shift+Down and Shift+Right select a rectangle of cells
      When the method "at" of "root" is called with 3
      And the user presses "Shift+Down"
      And the user presses "Shift+Right"
      And the method "shown" of "root" is called
      Then the property "out" of "root" is "2-9 c9 p q r s t u"

    Scenario: Shift+Down twice keeps the column it started in
      When the method "at" of "root" is called with 3
      And the user presses "Shift+Down"
      And the user presses "Shift+Down"
      And the method "shown" of "root" is called
      Then the property "out" of "root" is "1-13 c13 fdd0 p q fdd0 r fdd0 s fdd0 t u fdd1 y"

    Scenario: Shift+Left at the start of a row goes nowhere
      When the method "at" of "root" is called with 2
      And the user presses "Shift+Left"
      And the method "shown" of "root" is called
      Then the property "out" of "root" is "2-2 c2"
