# Sources:
#   https://doc.qt.io/qt-6/qtquick-input-focus.html
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#activeFocus-prop
#   https://doc.qt.io/qt-6/qml-qtquick-window.html#activeFocusItem-prop
#   https://doc.qt.io/qt-6/qml-qtquick-window.html#active-prop
# Verified on Qt 6.11.2 (qml6, offscreen; key input over its vnc platform).

Feature: Focus follows the active window
  Only the window the user types into has an item with active focus. The
  items remember who wants focus while the window is in the background.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property string active:
          Window.activeFocusItem ? Window.activeFocusItem.objectName : ""
        property bool nobody: Window.activeFocusItem === null
        property string keys: ""
        FocusScope {
          objectName: "scope"; focus: true
          Item {
            objectName: "field"; focus: true
            Keys.onPressed: (event) => root.keys += event.text
          }
        }
        Item { objectName: "other" }
        Shortcut { sequence: "Ctrl+S"; onActivated: root.keys += "save" }
      }
      """

  @qt-parity
  Scenario: A window in the background has no item with active focus
    When the window loses focus
    Then the property "activeFocus" of "field" is false
    And the property "activeFocus" of "scope" is false
    And the property "focus" of "field" is true
    And the property "focus" of "scope" is true
    And the property "nobody" of "root" is true

  @qt-parity
  Scenario: Active focus comes back with the window
    When the window loses focus
    And the window gains focus
    Then the property "activeFocus" of "field" is true
    And the property "activeFocus" of "scope" is true
    And the property "active" of "root" is "field"

  @qt-parity
  Scenario: Focus asked for in the background is given when the window is active
    When the window loses focus
    And the method "forceActiveFocus" of "other" is called
    Then the property "focus" of "other" is true
    And the property "activeFocus" of "other" is false
    And the property "focus" of "scope" is false
    When the window gains focus
    Then the property "activeFocus" of "other" is true
    And the property "active" of "root" is "other"

  @qt-parity
  Scenario: Keys and shortcuts do not reach a window in the background
    When the window loses focus
    And the user presses "A"
    And the user presses "Ctrl+S"
    And the window gains focus
    And the user presses "B"
    Then the property "keys" of "root" is "b"

  # Qt gives every window an invisible content item that has active focus
  # when no item of the application does, so its activeFocusItem is never
  # null in an active window. Ljós has no such item: with no focus item,
  # activeFocusItem is null, which is also what a QML author checks for.
  @qt-diverges
  Scenario: With no focus item, the active window's activeFocusItem is null
    When the property "focus" of "scope" is set to false
    Then the property "nobody" of "root" is true
