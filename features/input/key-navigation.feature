# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-keynavigation.html
#   https://doc.qt.io/qt-6/qtquick-input-focus.html
# Verified on Qt 6.11.2 (qml6, offscreen; key input over its vnc platform).

Feature: KeyNavigation
  KeyNavigation names the item an arrow key or Tab moves focus to. It acts
  after the item's Keys handlers, and only on a press nobody accepted.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property string active:
          Window.activeFocusItem ? Window.activeFocusItem.objectName : ""
        property string back: b.KeyNavigation.left ? b.KeyNavigation.left.objectName : ""
        property int order: a.KeyNavigation.priority
        Item {
          id: a
          objectName: "a"; focus: true
          KeyNavigation.right: b; KeyNavigation.down: c
          KeyNavigation.tab: c; KeyNavigation.backtab: b
          property bool take: false
          Keys.onRightPressed: (event) => event.accepted = take
        }
        Item { id: b; objectName: "b"; KeyNavigation.right: c }
        Item { KeyNavigation.right: a; Item { id: c; objectName: "c"; KeyNavigation.down: s } }
        FocusScope { id: s; objectName: "scope"; Item { objectName: "inside"; focus: true } }
      }
      """

  @qt-parity
  Scenario Outline: An arrow key moves focus to the item named
    When the user presses "<key>"
    Then the property "active" of "root" is "<item>"
    And the property "focus" of "a" is false

    Examples:
      | key   | item |
      | Right | b    |
      | Down  | c    |

  @qt-parity
  Scenario: Tab goes to the item KeyNavigation.tab names
    When the user presses "Tab"
    Then the property "active" of "root" is "c"

  @qt-parity
  Scenario: The way back is implied
    Then the property "back" of "root" is "a"
    When the user presses "Right"
    And the user presses "Left"
    Then the property "active" of "root" is "a"

  @qt-parity
  Scenario: A key without an item named does nothing
    When the user presses "Up"
    Then the property "active" of "root" is "a"

  @qt-parity
  Scenario: A Keys handler that accepts the key keeps focus where it is
    When the property "take" of "a" is set to true
    And the user presses "Right"
    Then the property "active" of "root" is "a"

  @qt-parity
  Scenario Outline: An item that cannot take focus is passed over
    When the property "<property>" of "b" is set to false
    And the user presses "Right"
    Then the property "active" of "root" is "c"

    Examples:
      | property |
      | visible  |
      | enabled  |

  @qt-parity
  Scenario: The search goes on through every item that cannot take focus
    When the property "visible" of "c" is set to false
    And the user presses "Down"
    Then the property "active" of "root" is "inside"

  @qt-parity
  Scenario: Focus stays when the item named cannot take it and names no other
    When the property "focus" of "c" is set to true
    And the property "visible" of "scope" is set to false
    And the user presses "Down"
    Then the property "active" of "root" is "c"

  @qt-parity
  Scenario: A parent's KeyNavigation is used when the focus item names nothing
    When the property "focus" of "c" is set to true
    And the user presses "Right"
    Then the property "active" of "root" is "a"

  @qt-parity
  Scenario: Moving to a focus scope gives its focus item active focus
    When the property "focus" of "c" is set to true
    And the user presses "Down"
    Then the property "active" of "root" is "inside"
    And the property "activeFocus" of "scope" is true

  @qt-parity
  Scenario: KeyNavigation acts after the item's own key handling by default
    Then the property "order" of "root" is 1

  # The oracle's keyboard sends Shift+Tab as Tab with Shift held, not as the
  # Backtab key a desktop sends, so it never asks for KeyNavigation.backtab.
  @ljos-only
  Scenario: Shift+Tab goes to the item KeyNavigation.backtab names
    When the user presses "Shift+Tab"
    Then the property "active" of "root" is "b"
