# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-shortcut.html
#   https://doc.qt.io/qt-6/qkeysequence.html#details
#   https://doc.qt.io/qt-6/qkeysequence.html#standard-shortcuts
#   https://doc.qt.io/qt-6/qml-qtquick-keys.html#shortcutOverride-signal
# Verified on Qt 6.11.2 (qml6, offscreen; key input over its vnc platform).

Feature: Shortcut
  A Shortcut acts on its key sequence wherever focus is in the window. It
  takes the press before any Keys handler, unless the focus item claims the
  key with shortcutOverride.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; focus: true
        property string log: ""
        property bool claim: false
        function note(event, what) {
          if (event.key < Qt.Key_Shift || event.key > Qt.Key_Alt) log += what
        }
        Keys.onShortcutOverride: (event) => event.accepted = claim
        Keys.onPressed: (event) => note(event, "pressed ")
        Keys.onReleased: (event) => note(event, "released ")
        Shortcut { objectName: "save"; sequence: "Ctrl+S"; onActivated: root.log += "save " }
        Shortcut { sequences: ["Ctrl+F", "F3"]; onActivated: root.log += "find " }
        Shortcut { sequence: StandardKey.Copy; onActivated: root.log += "copy " }
        Shortcut {
          objectName: "comment"; sequence: "Ctrl+K, Ctrl+C"
          onActivated: root.log += "comment "
        }
        Shortcut { sequence: "Ctrl+J"; onActivated: root.log += "join " }
        Shortcut { sequence: "Ctrl+J, Ctrl+L"; onActivated: root.log += "lines " }
        Item {
          visible: false; enabled: false
          Shortcut { sequence: "F2"; onActivated: root.log += "rename " }
        }
      }
      """

  @qt-parity
  Scenario: The shortcut takes the press; the release still reaches Keys
    When the user presses "Ctrl+S"
    Then the property "log" of "root" is "save released "

  @qt-parity
  Scenario Outline: A key that is no shortcut goes to Keys
    When the user presses "<chord>"
    Then the property "log" of "root" is "pressed released "

    Examples:
      | chord        |
      | S            |
      | Ctrl+Shift+S |
      | Alt+S        |

  @qt-parity
  Scenario Outline: Any of several sequences activates the shortcut
    When the user presses "<chord>"
    Then the property "log" of "root" is "find released "

    Examples:
      | chord  |
      | Ctrl+F |
      | F3     |

  @qt-parity
  Scenario: A standard key is the platform's sequence for it
    When the user presses "Ctrl+C"
    Then the property "log" of "root" is "copy released "

  @qt-parity
  Scenario: A sequence of two chords activates on the second
    When the user presses "Ctrl+K"
    And the user presses "Ctrl+C"
    Then the property "log" of "root" is "released comment released "

  @qt-parity
  Scenario: A wrong second chord is swallowed and ends the sequence
    When the user presses "Ctrl+K"
    And the user presses "Ctrl+S"
    Then the property "log" of "root" is "released released "
    When the user presses "Ctrl+S"
    Then the property "log" of "root" is "released released save released "

  @qt-parity
  Scenario: A whole sequence wins over a longer one that starts with it
    When the user presses "Ctrl+J"
    And the user presses "Ctrl+L"
    Then the property "log" of "root" is "join released pressed released "

  @qt-parity
  Scenario: A disabled shortcut leaves the key to Keys
    When the property "enabled" of "save" is set to false
    And the user presses "Ctrl+S"
    Then the property "log" of "root" is "pressed released "

  @qt-parity
  Scenario: The focus item can claim the key with shortcutOverride
    When the property "claim" of "root" is set to true
    And the user presses "Ctrl+S"
    Then the property "log" of "root" is "pressed released "

  @qt-parity
  Scenario: A shortcut works from inside a hidden, disabled item
    When the user presses "F2"
    Then the property "log" of "root" is "rename released "

  @qt-parity
  Scenario: A shortcut works with no focus item
    When the property "focus" of "root" is set to false
    And the user presses "Ctrl+S"
    Then the property "log" of "root" is "save "

  @qt-parity
  Scenario Outline: A shortcut's properties
    Then the property "<property>" of "<shortcut>" is <value>

    Examples:
      | shortcut | property     | value            |
      | save     | sequence     | "Ctrl+S"         |
      | save     | portableText | "Ctrl+S"         |
      | save     | nativeText   | "Ctrl+S"         |
      | comment  | portableText | "Ctrl+K, Ctrl+C" |
      | save     | enabled      | true             |
      | save     | autoRepeat   | true             |
      | save     | context      | 1                |
      | save     | sequences    | []               |

  # The oracle's input platform does not mark repeated presses as repeats.
  @ljos-only
  Scenario: A held key activates the shortcut again on each repeat
    When the user holds down "Ctrl+S" until it repeats 2 times
    Then the property "log" of "root" is "save save save released "

  @ljos-only
  Scenario: With autoRepeat false a held key activates the shortcut once
    When the property "autoRepeat" of "save" is set to false
    And the user holds down "Ctrl+S" until it repeats 2 times
    Then the property "log" of "root" is "save released "
