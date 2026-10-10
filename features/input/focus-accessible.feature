# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-accessible.html (focused, focusable)
#   https://doc.qt.io/qt-6/qtquick-input-focus.html
#   https://doc.qt.io/qt-6/qtquick-accessibility.html
# Verified on Qt 6.11.2 (qml6, offscreen).
# The Qt oracle cannot read the accessibility tree through QML alone, so the
# scenarios about the tree and about an assistive technology's requests run on
# Ljós only.

@a11y
Feature: Focus as an assistive technology sees it
  The item with active focus is the one a screen reader reads out, and a screen
  reader can move the focus itself.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        width: 200; height: 100
        FocusScope {
          objectName: "bar"
          focus: true
          Accessible.role: Accessible.ToolBar
          Accessible.name: "Tools"
          Rectangle {
            objectName: "save"
            focus: true
            Accessible.role: Accessible.Button
            Accessible.name: "Save"
          }
          Rectangle {
            objectName: "close"
            Accessible.role: Accessible.Button
            Accessible.name: "Close"
          }
        }
        Text { objectName: "hint"; text: "Ready"; Accessible.role: Accessible.StaticText }
      }
      """

  # Qt tells an assistive technology about active focus by itself; the property
  # only ever reads what the author assigned to it.
  @qt-parity
  Scenario: Accessible.focused holds what was assigned to it, not active focus
    Then the property "activeFocus" of "save" is true
    And the property "Accessible.focused" of "save" is false

  @ljos-only
  Scenario: The tree's focus is the item with active focus
    Then the accessibility tree's focus is the "button" named "Save"

  @ljos-only
  Scenario: The tree's focus moves with active focus
    Given an assistive technology is listening
    When the property "focus" of "close" is set to true
    Then the accessibility tree's focus is the "button" named "Close"

  @ljos-only
  Scenario: Around a focus item that is not exposed, the nearest exposed scope has the focus
    When the property "Accessible.ignored" of "save" is set to true
    Then the accessibility tree's focus is the "toolbar" named "Tools"

  @ljos-only
  Scenario: With no active focus the tree's focus is the window
    When the property "focus" of "bar" is set to false
    Then the accessibility tree's focus is the "window" named ""

  @ljos-only
  Scenario: An item that draws its own focus says so with Accessible.focused
    When the property "focus" of "bar" is set to false
    And the property "Accessible.focused" of "hint" is set to true
    Then the accessibility tree's focus is the "label" named "Ready"

  @ljos-only
  Scenario: An assistive technology gives an item the focus
    When an assistive technology focuses the "button" named "Close"
    Then the property "activeFocus" of "close" is true
    And the property "activeFocus" of "save" is false
    And the accessibility tree's focus is the "button" named "Close"

  @ljos-only
  Scenario: Focus given by an assistive technology takes the item's focus scope along
    When the property "focus" of "hint" is set to true
    And an assistive technology focuses the "button" named "Close"
    Then the property "activeFocus" of "bar" is true
    And the property "activeFocus" of "close" is true
    And the property "activeFocus" of "hint" is false

  @ljos-only
  Scenario: An item that is not focusable refuses the focus
    When the property "focus" of "hint" is set to true
    And an assistive technology focuses the "toolbar" named "Tools"
    Then the property "activeFocus" of "hint" is true
    And the property "activeFocus" of "bar" is false

  @ljos-only
  Scenario: A focus scope that takes the focus hands it to its own focus item
    When the property "focus" of "hint" is set to true
    And the property "Accessible.focusable" of "bar" is set to true
    And an assistive technology focuses the "toolbar" named "Tools"
    Then the property "activeFocus" of "save" is true

  @ljos-only
  Scenario: A disabled item refuses the focus
    When the property "enabled" of "close" is set to false
    And an assistive technology focuses the "button" named "Close"
    Then the property "activeFocus" of "save" is true
    And the property "activeFocus" of "close" is false
