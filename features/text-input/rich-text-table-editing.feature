# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#remove-method
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#paste-method
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#getText-method
# Verified on Qt 6.11.2 (qml6, offscreen and vnc; Tab with real keys on xcb).

@qt-parity
Feature: Editing the cells of a table in a rich TextEdit
  A tab typed or pasted into a cell stays in that cell, as any other
  character does: the table keeps its cells. remove() gives nothing back,
  as in a plain TextEdit. The table is "x", then a row of "pq" and "r"
  and a row of "s" and "tu", then "y". Its plain text marks a cell with
  U+FDD0 before it and the table's end with U+FDD1 after it.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 300; height: 200
        property string out
        function hex(s) {
          return Array.from(s, c => c.charCodeAt(0) > 127 || c.charCodeAt(0) < 32 ? c.charCodeAt(0).toString(16) : c).join(" ")
        }
        function all() { out = edit.length + " c" + edit.cursorPosition + " " + hex(edit.getText(0, edit.length)) }
        function tab(at) { src.selectAll(); src.copy(); edit.cursorPosition = at; edit.paste(); all() }
        function typing(at) { edit.forceActiveFocus(); edit.cursorPosition = at }
        function cut(a, b) { const r = edit.remove(a, b); all(); out = typeof r + " " + out }
        function selected(a, b) { edit.select(a, b); out = hex(edit.selectedText) }
        TextEdit { id: src; y: 150; text: "\t" }
        TextEdit {
          id: edit; objectName: "edit"
          textFormat: TextEdit.RichText; font.family: "Liberation Sans"; font.pixelSize: 20
          text: "x<table border='1'><tr><td>pq</td><td>r</td></tr><tr><td>s</td><td>tu</td></tr></table>y"
        }
      }
      """

  Rule: A tab in a cell is a character of that cell

    Scenario Outline: A tab pasted at <at> stays where it is put
      When the method "tab" of "root" is called with <at>
      Then the property "out" of "root" is "<text>"

      Examples:
        | at | text |
        | 3  | 14 c4 x fdd0 p 9 q fdd0 r fdd0 s fdd0 t u fdd1 y |
        | 6  | 14 c7 x fdd0 p q fdd0 r 9 fdd0 s fdd0 t u fdd1 y |
        | 11 | 14 c12 x fdd0 p q fdd0 r fdd0 s fdd0 t u 9 fdd1 y |
        | 0  | 14 c1 9 x fdd0 p q fdd0 r fdd0 s fdd0 t u fdd1 y |

    Scenario: A tab in a cell is selected with the cell's other text
      When the method "tab" of "root" is called with 3
      And the method "selected" of "root" is called with 2, 5
      Then the property "out" of "root" is "p 9 q"

    # The oracle's vnc keys carry no text for Tab, so qml6 there types
    # nothing; with real keys (xcb, xdotool) it types the tab as here.
    @ljos-only
    Scenario Outline: Tab typed at <at> types a tab in that cell
      When the method "typing" of "root" is called with <at>
      And the user presses "Tab"
      And the method "all" of "root" is called
      Then the property "out" of "root" is "<text>"

      Examples:
        | at | text |
        | 3  | 14 c4 x fdd0 p 9 q fdd0 r fdd0 s fdd0 t u fdd1 y |
        | 4  | 14 c5 x fdd0 p q 9 fdd0 r fdd0 s fdd0 t u fdd1 y |
        | 12 | 14 c13 x fdd0 p q fdd0 r fdd0 s fdd0 t u fdd1 9 y |

    Scenario: Shift+Tab in a cell types nothing
      When the method "typing" of "root" is called with 6
      And the user presses "Shift+Tab"
      And the method "all" of "root" is called
      Then the property "out" of "root" is "13 c6 x fdd0 p q fdd0 r fdd0 s fdd0 t u fdd1 y"

  Rule: remove() gives nothing back

    Scenario Outline: remove(<a>, <b>) gives undefined and leaves <text>
      When the method "cut" of "root" is called with <a>, <b>
      Then the property "out" of "root" is "undefined <text>"

      Examples:
        | a | b  | text |
        | 2 | 3  | 12 c0 x fdd0 q fdd0 r fdd0 s fdd0 t u fdd1 y |
        | 0 | 1  | 12 c0 fdd0 p q fdd0 r fdd0 s fdd0 t u fdd1 y |
