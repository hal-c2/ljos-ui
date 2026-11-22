# Sources:
#   docs/ARCHITECTURE.md sections 19.4 and 19.6 (`app_press`, `app_type`, `app_status`)
#   https://doc.qt.io/qt-6/qml-qtquick-window.html#active-prop
# The inspector is a Ljós extension: Qt has nothing to compare it with.

@ljos-only @desktop
Feature: An agent is told when the keys it presses would reach no window
  Keys go to the window that has the keyboard, as a person's do: one in
  the background hears none of them. `app_press` and `app_type` on a window that
  is not active are refused before anything is clicked, so that an agent
  is never told it pressed keys that went nowhere. `app_status` says whether
  the window has the keyboard, which an agent can wait for after giving
  it the focus.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Window {
        id: root
        objectName: "root"
        width: 200; height: 100; visible: true
        property int keys: 0
        property int clicks: 0
        Item {
          focus: true
          Keys.onPressed: root.keys += 1
        }
        MouseArea {
          objectName: "pad"
          anchors.fill: parent
          onPressed: root.clicks += 1
        }
      }
      """

  Scenario: The status says whether the window has the keyboard
    When the agent calls the tool "app_status"
    Then the answer's "/windows/0/active" is true
    When the window loses focus
    And the agent calls the tool "app_status"
    Then the answer's "/windows/0/active" is false
    And the answer's text contains "the window is not active"

  Scenario: Keys pressed into a window in the background are refused
    When the window loses focus
    And the agent calls the tool "app_press" with:
      """json
      {"keys": "Return"}
      """
    Then the agent is refused with "not_actionable"
    And the refusal mentions "not active"
    And the property "keys" of "root" is 0

  Scenario: Text typed into a window in the background is refused
    When the window loses focus
    And the agent calls the tool "app_type" with:
      """json
      {"text": "hello"}
      """
    Then the agent is refused with "not_actionable"
    And the refusal mentions "not active"
    And the property "keys" of "root" is 0

  Scenario Outline: <tool> with a target clicks nothing in the background
    When the window loses focus
    And the agent calls the tool "<tool>" with:
      """json
      {<what>, "target": "#pad"}
      """
    Then the agent is refused with "not_actionable"
    And the refusal mentions "not active"
    And the property "clicks" of "root" is 0

    Examples:
      | tool      | what             |
      | app_press | "keys": "Return" |
      | app_type  | "text": "hello"  |

  Scenario: Keys reach the window once it has the keyboard again
    When the window loses focus
    And the window gains focus
    And the agent calls the tool "app_press" with:
      """json
      {"keys": "Return"}
      """
    Then the property "keys" of "root" is 1
