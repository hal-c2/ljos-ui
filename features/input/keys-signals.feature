# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-keys.html#signals
# Verified on Qt 6.11.2 (qml6, offscreen; key input over its vnc platform).

Feature: Keys signals for single keys
  Keys has a signal for each of the keys an application most often handles,
  so that a handler need not compare key codes.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        focus: true
        property string got: ""
        Keys.onDigit0Pressed: got = "digit0"; Keys.onDigit9Pressed: got = "digit9"
        Keys.onLeftPressed: got = "left"; Keys.onRightPressed: got = "right"
        Keys.onUpPressed: got = "up"; Keys.onDownPressed: got = "down"
        Keys.onTabPressed: got = "tab"; Keys.onBacktabPressed: got = "backtab"
        Keys.onReturnPressed: got = "return"; Keys.onEnterPressed: got = "enter"
        Keys.onEscapePressed: got = "escape"; Keys.onSpacePressed: got = "space"
        Keys.onDeletePressed: got = "delete"; Keys.onAsteriskPressed: got = "asterisk"
        Keys.onMenuPressed: got = "menu"; Keys.onBackPressed: got = "back"
        Keys.onSelectPressed: got = "select"; Keys.onCancelPressed: got = "cancel"
        Keys.onYesPressed: got = "yes"; Keys.onNoPressed: got = "no"
        Keys.onVolumeUpPressed: got = "volumeUp"
        Keys.onVolumeDownPressed: got = "volumeDown"
        Keys.onPressed: got = "other"
      }
      """

  @qt-parity
  Scenario Outline: The key has a signal of its own
    When the user presses "<chord>"
    Then the property "got" of "root" is "<signal>"

    Examples:
      | chord  | signal   |
      | 0      | digit0   |
      | 9      | digit9   |
      | Left   | left     |
      | Right  | right    |
      | Up     | up       |
      | Down   | down     |
      | Tab    | tab      |
      | Return | return   |
      | Escape | escape   |
      | Space  | space    |
      | Delete | delete   |
      | *      | asterisk |
      | A      | other    |

  # Keys the oracle's keyboard does not have.
  @ljos-only
  Scenario Outline: Keys of a full keyboard and of a remote control
    When the user presses "<chord>"
    Then the property "got" of "root" is "<signal>"

    Examples:
      | chord       | signal     |
      | Shift+Tab   | backtab    |
      | Enter       | enter      |
      | Menu        | menu       |
      | Back        | back       |
      | Select      | select     |
      | Cancel      | cancel     |
      | Yes         | yes        |
      | No          | no         |
      | Volume Up   | volumeUp   |
      | Volume Down | volumeDown |
