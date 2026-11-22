# Sources:
#   docs/ARCHITECTURE.md (sections 19.5, `ljos observe`, and 19.7, the
#   calls a second the inspector answers)
#   https://www.rfc-editor.org/rfc/rfc6585#section-4 (429 Too Many Requests)
# The observer is a Ljós tool: Qt has nothing to compare it with.

@ljos-only @desktop
Feature: The observer shows the object picked while the application is asked too often
  The application answers at most so many calls a second, and turns the
  rest away. An object picked on the Objects page whose values were turned
  away is asked of again in a while: an application at rest tells the
  observer of no change that would ask again. The answer about an object
  picked before, which comes late, is not shown in its place, and that
  one's values are put away at once, so none of them is pressed for it.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        width: 200; height: 100; color: "white"
        // Not items, so that no picture or watch of one is asked of them:
        // their values are the only thing the observer asks.
        QtObject { id: a; property string first: "a" }
        QtObject { id: b; property string second: "b" }
      }
      """

  Scenario: An object picked while every call is turned away shows its values once the application answers
    When the application is observed
    And the observer's "text=Objects" is clicked
    And the observer's "role=listitem[name~=\"#a$\"]" is clicked
    Then the observer has "role=button[name=\"first\"]"
    When the observer's "role=listitem[name~=\"#b$\"]" is clicked while the application turns every call away
    Then the observer has "role=button[name=\"second\"]"
    And the observer does not have "role=button[name=\"first\"]"

  Scenario: The values of the object picked before are put away before those of the one picked come
    When the application is observed
    And the observer's "text=Objects" is clicked
    And the observer's "role=listitem[name~=\"#a$\"]" is clicked
    Then the observer has "role=button[name=\"first\"]"
    When the observer's "role=listitem[name~=\"#b$\"]" is clicked while the application turns every call away, and its "role=button[name=\"first\"]" is put away meanwhile
    Then the observer has "role=button[name=\"second\"]"

  Scenario: The values of an object picked before are not shown when they come after those of the one picked since
    When the application is observed
    And the observer's "text=Objects" is clicked
    And the observer's "role=listitem[name~=\"#a$\"]" is clicked while the application turns calls away for a moment, and its "role=listitem[name~=\"#b$\"]" once it answers again
    Then the observer has "role=button[name=\"second\"]"
    And the observer does not have "role=button[name=\"first\"]"
