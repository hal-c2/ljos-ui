# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-shortcut.html
#   https://doc.qt.io/qt-6/qml-qtquick-keys.html#shortcutOverride-signal
#   https://doc.qt.io/qt-6/qkeysequence.html#standard-shortcuts
# Verified on Qt 6.11.2 (qml6, offscreen; key input over its vnc platform).

@qt-parity
Feature: An editor and a Shortcut on the same key
  An editor with the keyboard keeps the common editing keys from a window
  Shortcut: the clipboard, undo and redo, Select All, the cursor moves and
  selections, and the keys that type, unmodified or with Shift. Any other
  key it would act on goes to a Shortcut first, and to the editor only
  when no enabled Shortcut wants it.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Item {
        id: root; objectName: "root"; width: 300; height: 300
        property string log: ""
        Shortcut { objectName: "kill"; sequence: "Ctrl+K"; onActivated: root.log += "kill " }
        Shortcut {
          objectName: "words"; sequences: ["Ctrl+Backspace", "Ctrl+Del"]
          onActivated: root.log += "words "
        }
        Shortcut {
          objectName: "common"
          sequences: ["Ctrl+C", "Ctrl+X", "Ctrl+V", "Ctrl+Z", "Ctrl+Shift+Z", "Ctrl+A",
            "Left", "Right", "Home", "End", "Shift+Left", "Ctrl+Left", "Ctrl+Right",
            "Ctrl+Shift+Left", "Backspace", "Del", "D", "Shift+D"]
          onActivated: root.log += "common "
        }
        TextInput { objectName: "input"; focus: true; width: 200; text: "abc def" }
        TextEdit { objectName: "edit"; y: 30; width: 200; height: 40; text: "abc def" }
        TextField { objectName: "field"; y: 80; width: 200; text: "abc def" }
        TextArea { objectName: "area"; y: 140; width: 200; height: 60; text: "abc def" }
      }
      """

  Rule: A Shortcut takes a key that is not a common editing key

    # Ctrl+K is DeleteEndOfLine in the Linux column; the oracle has only
    # Windows's key bindings, where Ctrl+K is no editing key at all.
    @ljos-only
    Scenario Outline: Ctrl+K in a <editor> goes to the Shortcut under Linux's key bindings
      Given the application uses the key bindings of "Linux"
      When the method "forceActiveFocus" of "<editor>" is called
      And the property "cursorPosition" of "<editor>" is set to 3
      And the user presses "Ctrl+K"
      Then the property "log" of "root" is "kill "
      And the property "text" of "<editor>" is "abc def"

      Examples:
        | editor |
        | input  |
        | edit   |
        | field  |
        | area   |

    @ljos-only
    Scenario Outline: Ctrl+K in a <editor> deletes to the end of the line when no Shortcut wants it
      Given the application uses the key bindings of "Linux"
      When the property "enabled" of "kill" is set to false
      And the method "forceActiveFocus" of "<editor>" is called
      And the property "cursorPosition" of "<editor>" is set to 3
      And the user presses "Ctrl+K"
      Then the property "log" of "root" is ""
      And the property "text" of "<editor>" is "abc"

      Examples:
        | editor |
        | input  |
        | edit   |
        | field  |
        | area   |

    Scenario Outline: <key> in a <editor> goes to the Shortcut
      When the method "forceActiveFocus" of "<editor>" is called
      And the property "cursorPosition" of "<editor>" is set to 3
      And the user presses "<key>"
      Then the property "log" of "root" is "words "
      And the property "text" of "<editor>" is "abc def"

      Examples:
        | editor | key            |
        | input  | Ctrl+Backspace |
        | input  | Ctrl+Del       |
        | edit   | Ctrl+Backspace |
        | edit   | Ctrl+Del       |
        | field  | Ctrl+Backspace |
        | area   | Ctrl+Del       |

    Scenario Outline: <key> in a <editor> deletes a word when no Shortcut wants it
      When the property "enabled" of "words" is set to false
      And the method "forceActiveFocus" of "<editor>" is called
      And the property "cursorPosition" of "<editor>" is set to 5
      And the user presses "<key>"
      Then the property "log" of "root" is ""
      And the property "text" of "<editor>" is "<text>"

      Examples:
        | editor | key            | text   |
        | input  | Ctrl+Backspace | abc ef |
        | edit   | Ctrl+Del       | abc d  |
        | field  | Ctrl+Del       | abc d  |
        | area   | Ctrl+Backspace | abc ef |

  Rule: The common editing keys stay with the editor

    Scenario Outline: <key> in a <editor> moves the cursor, not the Shortcut
      When the method "forceActiveFocus" of "<editor>" is called
      And the property "cursorPosition" of "<editor>" is set to 3
      And the user presses "<key>"
      Then the property "log" of "root" is ""
      And the property "cursorPosition" of "<editor>" is <cursor>
      And the property "selectedText" of "<editor>" is "<selected>"

      Examples:
        | editor | key              | cursor | selected |
        | input  | Left             | 2      |          |
        | input  | Right            | 4      |          |
        | input  | Home             | 0      |          |
        | input  | End              | 7      |          |
        | input  | Shift+Left       | 2      | c        |
        | input  | Ctrl+Left        | 0      |          |
        | input  | Ctrl+Right       | 4      |          |
        | input  | Ctrl+Shift+Left  | 0      | abc      |
        | input  | Ctrl+A           | 7      | abc def  |
        | edit   | Left             | 2      |          |
        | edit   | Ctrl+Right       | 4      |          |
        | edit   | Ctrl+Shift+Left  | 0      | abc      |
        | edit   | Ctrl+A           | 7      | abc def  |
        | field  | Shift+Left       | 2      | c        |
        | field  | Ctrl+Left        | 0      |          |
        | area   | End              | 7      |          |
        | area   | Ctrl+A           | 7      | abc def  |

    Scenario Outline: <key> in a <editor> edits the text, not the Shortcut
      When the method "forceActiveFocus" of "<editor>" is called
      And the property "cursorPosition" of "<editor>" is set to 3
      And the user presses "<key>"
      Then the property "log" of "root" is ""
      And the property "text" of "<editor>" is "<text>"

      Examples:
        | editor | key       | text     |
        | input  | Backspace | ab def   |
        | input  | Del       | abcdef   |
        | input  | D         | abcd def |
        | input  | Shift+D   | abcD def |
        | edit   | Backspace | ab def   |
        | edit   | D         | abcd def |
        | field  | Del       | abcdef   |
        | area   | Shift+D   | abcD def |

    Scenario Outline: The clipboard, undo and redo in a <editor> stay with the editor
      When the method "forceActiveFocus" of "<editor>" is called
      And the user presses "Ctrl+A"
      And the user presses "Ctrl+C"
      And the user presses "Ctrl+X"
      Then the property "text" of "<editor>" is ""
      When the user presses "Ctrl+V"
      And the user presses "Ctrl+V"
      Then the property "text" of "<editor>" is "abc defabc def"
      When the user presses "Ctrl+Z"
      Then the property "text" of "<editor>" is "abc def"
      When the user presses "Ctrl+Shift+Z"
      Then the property "text" of "<editor>" is "abc defabc def"
      And the property "log" of "root" is ""

      Examples:
        | editor |
        | input  |
        | edit   |
        | field  |
        | area   |
