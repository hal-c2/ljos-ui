# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-keyevent.html
#   https://doc.qt.io/qt-6/qt.html#Key-enum
#   https://doc.qt.io/qt-6/qt.html#KeyboardModifier-enum
#   https://doc.qt.io/qt-6/qkeysequence.html#standard-shortcuts
# Verified on Qt 6.11.2 (qml6, offscreen; key input over its vnc platform).

Feature: KeyEvent
  What a Keys handler is told about a key press.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        focus: true
        property int key: 0
        property string text: "?"
        property int modifiers: -1
        property int count: 0
        property bool copy: false
        property int presses: 0
        property int repeats: 0
        property int releases: 0
        Keys.onPressed: (event) => {
          key = event.key; text = event.text; modifiers = event.modifiers
          count = event.count; copy = event.matches(StandardKey.Copy)
          presses++
          if (event.isAutoRepeat) repeats++
        }
        Keys.onReleased: releases++
      }
      """

  @qt-parity
  Scenario Outline: The key is a Qt.Key value
    When the user presses "<chord>"
    Then the property "key" of "root" is <key>

    Examples:
      | chord     | key      |
      | A         | 65       |
      | Z         | 90       |
      | 1         | 49       |
      | Space     | 32       |
      | Escape    | 16777216 |
      | Tab       | 16777217 |
      | Backspace | 16777219 |
      | Return    | 16777220 |
      | Insert    | 16777222 |
      | Delete    | 16777223 |
      | Home      | 16777232 |
      | End       | 16777233 |
      | Left      | 16777234 |
      | Up        | 16777235 |
      | Right     | 16777236 |
      | Down      | 16777237 |
      | PgUp      | 16777238 |
      | PgDown    | 16777239 |
      | F1        | 16777264 |
      | F12       | 16777275 |

  # The oracle's keyboard has no keypad, Backtab or Menu key of its own.
  @ljos-only
  Scenario Outline: Keys of a full keyboard
    When the user presses "<chord>"
    Then the property "key" of "root" is <key>

    Examples:
      | chord     | key      |
      | Enter     | 16777221 |
      | Shift+Tab | 16777218 |
      | Menu      | 16777301 |

  @qt-parity
  Scenario Outline: The modifiers held are a Qt.KeyboardModifier set
    When the user presses "<chord>"
    Then the property "modifiers" of "root" is <modifiers>
    And the property "key" of "root" is 65

    Examples:
      | chord        | modifiers |
      | A            | 0         |
      | Shift+A      | 33554432  |
      | Ctrl+A       | 67108864  |
      | Alt+A        | 134217728 |
      | Ctrl+Shift+A | 100663296 |

  @qt-parity
  Scenario Outline: The text is what the key types
    When the user presses "<chord>"
    Then the property "text" of "root" is "<text>"

    Examples:
      | chord   | text |
      | A       | a    |
      | Shift+A | A    |
      | 7       | 7    |

  # The oracle's input platform reports a NUL character for these keys.
  @ljos-only
  Scenario Outline: A key that types nothing has no text
    When the user presses "<chord>"
    Then the property "text" of "root" is ""

    Examples:
      | chord  |
      | Escape |
      | Left   |
      | F1     |

  @qt-parity
  Scenario: A modifier key is a press of its own
    When the user presses "Shift+A"
    Then the property "presses" of "root" is 2
    And the property "releases" of "root" is 2

  @qt-parity
  Scenario: A press counts as one key and is not a repeat
    When the user presses "A"
    Then the property "count" of "root" is 1
    And the property "repeats" of "root" is 0

  @qt-parity
  Scenario: matches tells a standard key from the same letter alone
    When the user presses "Ctrl+C"
    Then the property "copy" of "root" is true
    When the user presses "C"
    Then the property "copy" of "root" is false

  # The oracle's input platform does not mark repeated presses as repeats.
  @ljos-only
  Scenario: A key held down repeats its press and is released once
    When the user holds down "A" until it repeats 3 times
    Then the property "presses" of "root" is 4
    And the property "repeats" of "root" is 3
    And the property "releases" of "root" is 1
