# Sources:
#   docs/ARCHITECTURE.md (section 19.5, `ljos observe`)
#   https://www.w3.org/WAI/ARIA/apg/patterns/tabs/ (the keys of a list of
#   tabs: one stop for the list, the arrows between tabs, Space or Enter
#   to show the one with focus)
# The observer is a Ljós tool: Qt has nothing to compare it with.

@ljos-only @desktop
Feature: The observer's pages are chosen from the keyboard
  Every page of the observer is one key press away, as it is one click
  away. Tab reaches the list of pages at the tab of the page shown; the
  arrow keys move between the tabs, past the last back to the first; Space
  or Return shows the page whose tab has focus and asks the application
  for what it shows, as a click does.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        width: 200; height: 100; color: "white"
      }
      """

  Scenario: Tab reaches the tab of the page shown
    When the application is observed
    And the observer is given the keys "Tab"
    Then the observer has "role=tab[name=\"Overview\"][focused]"

  Scenario: The arrows move between the tabs and Return shows a page
    When the application is observed
    And the observer is given the keys "Tab Right Right Right"
    Then the observer has "role=tab[name=\"Objects\"][focused]"
    And the observer does not have "role=listitem[name=\"Scenario\"]"
    When the observer is given the keys "Return"
    Then the observer has "role=listitem[name=\"Scenario\"]"

  Scenario: Left from the first tab is the last, and Space shows it
    When the application is observed
    And the observer is given the keys "Tab Left"
    Then the observer has "role=tab[name=\"Audit\"][focused]"
    When the observer is given the keys "Space"
    Then the observer shows "Not run"
