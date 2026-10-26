# Sources:
#   docs/ARCHITECTURE.md (section 19.4, the inspector's tools)
#   docs/design/README.md (the design rules and how each is checked)
#   https://www.w3.org/TR/WCAG22/#reflow
# The inspector is a Ljós extension: Qt has nothing to compare it with.

@ljos-only @headless
Feature: An agent audits the design of the running application
  `app_audit` runs the design audit on what the application shows and
  answers with each finding: the rule's code, severity, fix and guide,
  the item's role, name and a locator the other tools take, where in the
  QML it was made, its box in the window, what was measured, and at
  which settings it shows. It can look with the text larger, the window
  smaller, or in a dark or high contrast scheme, and puts every setting
  back afterwards. The checks that act (focus each stop, press Escape,
  toggle controls, open popups) run the application's own handlers, whose
  effects are real, so they run only when asked with `act`, which needs
  the `input` capability; without it the answer names the codes they
  would have checked.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Rectangle {
        objectName: "root"
        width: 400; height: 400
        color: "white"
        Row {
          objectName: "row"
          spacing: 8
          Button { text: "Open the file" }
          Button { text: "Save the file" }
          Button { text: "Close the file" }
        }
        Button { objectName: "lost"; x: 520; y: 200; text: "Help" }
      }
      """
    And the inspector is connected

  Scenario: A finding names its rule, its item and where it was made
    When the agent calls the tool "app_audit"
    Then the answer's "/counts/error" is 1
    And the answer's "/findings/0/code" is "layout-offscreen"
    And the answer's "/findings/0/locator" is "#lost"
    And the answer's "/findings/0/role" is "button"
    And the answer's "/findings/0/source" matches ":14:"
    And the answer's "/findings/0/guide" matches "^docs/design/.+#layout-offscreen$"
    And the answer's "/findings/0/bounds/x" is 520

  Scenario: Larger text pushes the row off the window
    When the agent calls the tool "app_audit" with:
      """json
      {"matrix": "font-scale=2"}
      """
    Then the answer's "/findings" has 2 entries
    And the answer's "/findings/0/name" is "Close the file"
    And the answer's "/findings/0/points/0" is "font-scale=2"

  Scenario: Every setting is put back after a full audit
    When the agent calls the tool "app_audit" with:
      """json
      {"matrix": "full"}
      """
    Then the property "width" of "row" is at most 400
    And the property "width" of "root" is 400

  Scenario: Only what is inside an item
    When the agent calls the tool "app_audit" with:
      """json
      {"root": "#row", "matrix": "font-scale=2"}
      """
    Then the answer's "/findings" has 1 entries
    And the answer's "/findings/0/name" is "Close the file"

  Scenario: An unknown rule is refused
    When the agent calls the tool "app_audit" with:
      """json
      {"rules": ["layout-nowhere"]}
      """
    Then the agent is refused with "invalid_argument"

  Scenario: The checks that act are left out unless asked, and named
    When the agent calls the tool "app_audit"
    Then the answer's "/notChecked" has 12 entries
    And the answer's "/notChecked/0" is "color-state-only"

  Scenario: Acting on the application needs the input capability
    When the agent calls the tool "app_audit" with:
      """json
      {"act": true}
      """
    Then the agent is refused with "capability_denied"

  Scenario: Asked to act, with input granted, every rule is checked
    Given the inspector is connected with the capabilities "inspect,input"
    When the agent calls the tool "app_audit" with:
      """json
      {"act": true}
      """
    Then the answer's "/notChecked" has 0 entries
