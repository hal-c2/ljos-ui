# Sources:
#   docs/ARCHITECTURE.md (principle 6: 0 % CPU when idle; sections 14 and
#   19.5)
#   docs/design/README.md (the tools)
#   https://www.w3.org/TR/WCAG22/#contrast-minimum
# The observer is a Ljós tool: Qt has nothing to compare it with.

@ljos-only @desktop
Feature: The observer audits what an application shows when asked
  The observer's Audit page runs the design audit (`app_audit`) of the
  observed application and lists what it found, grouped by rule. It audits
  only when Run audit is pressed: opening the page, or the application
  changing, asks nothing. The rules come from the audit's own answer, so a
  rule added to it is listed without a change to the page. The checks that
  act press keys and toggle controls in the application and run its
  handlers, so they are behind a switch of their own, off until the user
  turns it on, and the page says how many were not run.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        width: 320; height: 120; color: "white"
        Text {
          objectName: "faint"
          text: "Barely readable"
          color: "#d8d8d8"
          anchors.centerIn: parent
        }
      }
      """

  Scenario: The page asks nothing until it is told to
    When the application is observed
    And the observer's "text=Audit" is clicked
    Then the observer shows "Not run"
    And the observer has "role=button[name=\"Run audit\"]"

  Scenario: Running the audit lists the findings by rule with the fix
    When the application is observed
    And the observer's "text=Audit" is clicked
    And the observer's "role=button[name=\"Run audit\"]" is clicked
    Then the observer has "text=color-text-contrast"
    And the observer does not show "Not run"

  Scenario: Picking a finding shows its fix and where the guide is
    When the application is observed
    And the observer's "text=Audit" is clicked
    And the observer's "role=button[name=\"Run audit\"]" is clicked
    And the observer's "role=listitem" is clicked
    Then the observer has "role=button[name=\"Open guide\"]"
    And the observer shows "docs/design/color.md#color-text-contrast"

  Scenario: The checks that act are off until the user turns them on
    When the application is observed
    And the observer's "text=Audit" is clicked
    Then the observer has "role=checkbox[name=\"Acting checks\"]"
    When the observer's "role=button[name=\"Run audit\"]" is clicked
    Then the observer shows "12 acting checks skipped"

  Scenario: The page offers the areas the rules have and passes its own audit
    When the application is observed
    And the observer's "text=Audit" is clicked
    Then the observer has "role=combobox[name=\"Rules to audit\"]"
    And the observer passes its own audit

  Scenario: After an audit the page and the application are at rest
    When the application is observed
    And the observer's "text=Audit" is clicked
    And the observer's "role=button[name=\"Run audit\"]" is clicked
    And 2000 milliseconds pass
    And the work of the application and its observer is counted
    And 10000 milliseconds pass
    Then the application and its observer did no work since it was counted
