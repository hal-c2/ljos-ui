# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#selectByMouse-prop
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#persistentSelection-prop
#   https://doc.qt.io/qt-6/qml-qtquick-controls-menu.html#popup-method
# Verified on Qt 6.11.2 (qml6, offscreen; pointer input over its vnc platform).

@qt-parity
Feature: Selecting text with the pointer and copying it
  Read-only text a reader can select is a TextEdit with selectByMouse. A
  double click selects a word, a triple click the paragraph with the line
  break that ends it. A right-click leaves the selection alone, and so
  does a menu or a popup that takes the keyboard from the editor, so the
  menu can copy what was selected. Focus that moves to another item drops
  the selection, unless persistentSelection keeps it. The numbers are
  those of the bundled font, Liberation Sans, at 14 pixels.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        id: root; objectName: "root"; width: 300; height: 300
        property string held: ""
        property string shown: ""
        function choose(n) { const t = n === "edit" ? edit : input; t.forceActiveFocus(); t.select(6, 11) }
        TextEdit {
          id: edit; objectName: "edit"; width: 280; height: 40; font { family: "Liberation Sans"; pixelSize: 14 }
          text: "hello brave world\nsecond line here"; readOnly: true; selectByMouse: true
          TapHandler { acceptedButtons: Qt.RightButton; onTapped: { root.held = edit.selectedText; menu.popup() } }
        }
        TextEdit {
          objectName: "rich"; y: 50; width: 100; height: 60; font { family: "Liberation Sans"; pixelSize: 14 }
          wrapMode: TextEdit.Wrap; textFormat: TextEdit.RichText; readOnly: true; selectByMouse: true
          text: "<p>one <b>two</b> three four five six</p><p>seven</p>"
        }
        TextInput { id: input; objectName: "input"; y: 120; width: 200; height: 30; text: "hello brave world"; font { family: "Liberation Sans"; pixelSize: 14 } }
        TextEdit { objectName: "other"; y: 160; width: 200; height: 30; text: "other" }
        Menu {
          id: menu; objectName: "menu"; onOpened: root.shown = edit.selectedText
          MenuItem { objectName: "copy"; text: "Copy"; onTriggered: edit.copy() }
        }
        Popup { objectName: "popup"; focus: true; y: 200; width: 50; height: 50 }
      }
      """

  Rule: Quick clicks select a word, then a paragraph

    Scenario: A double click selects the word under the pointer
      When the user double-clicks at 50, 8
      Then the property "selectedText" of "edit" is "brave"
      And the property "activeFocus" of "edit" is true

    Scenario: A triple click selects the paragraph and the line break after it
      When the user triple-clicks at 50, 8
      Then the property "selectedText" of "edit" is "hello brave world\n"
      And the property "cursorPosition" of "edit" is 18

    Scenario: A triple click on the last paragraph selects it to the end
      When the user triple-clicks at 50, 25
      Then the property "selectedText" of "edit" is "second line here"
      And the property "cursorPosition" of "edit" is 34

    Scenario: A triple click on a wrapped line selects its whole paragraph
      When the user triple-clicks at 10, 75
      Then the property "selectedText" of "rich" is "one two three four five six "
      And the property "cursorPosition" of "rich" is 28

    Scenario: A triple click in a TextInput selects all of it
      When the user triple-clicks at 50, 128
      Then the property "selectedText" of "input" is "hello brave world"
      And the property "cursorPosition" of "input" is 17

    Scenario: Without selectByMouse quick clicks select nothing
      When the property "selectByMouse" of "edit" is set to false
      And the user triple-clicks at 50, 8
      Then the property "selectedText" of "edit" is ""

  Rule: A right-click and its menu leave the selection to be copied

    Scenario Outline: A right-click <where> keeps the selection
      When the user double-clicks at 50, 8
      And the user clicks the right button at <x>, <y>
      Then the property "held" of "root" is "brave"
      And the property "shown" of "root" is "brave"
      And the property "selectedText" of "edit" is "brave"

      Examples:
        | where             | x   | y  |
        | on it             | 50  | 8  |
        | beside it         | 150 | 25 |

    Scenario: The menu's Copy puts the selection on the clipboard
      When the user double-clicks at 50, 8
      And the user clicks the right button at 50, 8
      And the method "click" of "copy" is called
      Then the clipboard holds "brave"

    Scenario: The selection is still there when the menu closes
      When the user double-clicks at 50, 8
      And the user clicks the right button at 50, 8
      And the user presses "Escape"
      And 300 milliseconds pass
      Then the property "activeFocus" of "edit" is true
      And the property "selectedText" of "edit" is "brave"

    Scenario: Ctrl+C copies what a double click selected in read-only text
      When the user double-clicks at 50, 8
      And the user presses "Ctrl+C"
      Then the clipboard holds "brave"

  Rule: Focus that goes to a popup keeps the selection, elsewhere drops it

    Scenario Outline: Focus taken from <editor> by <taker>
      When the method "choose" of "root" is called with <editor>
      And the method <how> of <taker> is called
      And 100 milliseconds pass
      Then the property "selectedText" of <editor> is <selected>

      Examples:
        | editor  | how                | taker   | selected |
        | "edit"  | "forceActiveFocus" | "other" | ""       |
        | "edit"  | "popup"            | "menu"  | "brave"  |
        | "edit"  | "open"             | "popup" | "brave"  |
        | "input" | "forceActiveFocus" | "other" | ""       |
        | "input" | "popup"            | "menu"  | "brave"  |
        | "input" | "open"             | "popup" | "brave"  |
