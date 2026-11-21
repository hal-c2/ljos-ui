# Sources:
#   docs/ARCHITECTURE.md (section 19.5, `ljos observe`)
# The observer is a Ljós tool: Qt has nothing to compare it with.

@ljos-only @desktop
Feature: The observer fits a narrow window
  The observer is shown wherever there is room for it: a window of its own,
  a dock beside a page, a sheet on a phone. In a wide window its pages are
  listed beside the page shown. In a narrow one the list folds into tabs
  above the page, so the page has the whole width. What a page shows
  then wraps, elides or takes fewer columns. It is never cut off at the
  window's edge, and no two words are drawn over each other: the
  observer's own audit, which measures the words each text paints, finds
  none of that.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        width: 200; height: 100; color: "white"
      }
      """

  Scenario: A wide observer lists its pages beside the page
    When the application is observed
    And the observer's window is 1200 by 800 pixels
    Then the observer's "role=tab[name=\"Frames\"]" lies left of its "role=heading[name=\"Overview\"]"

  Scenario Outline: A narrow observer shows its pages as tabs above the page
    When the application is observed
    And the observer's window is <width> by 800 pixels
    Then the observer's "role=tab[name=\"Frames\"]" lies above its "role=heading[name=\"Overview\"]"
    And every "role=tab" in the observer lies within the width of its window
    And no two "role=tab" in the observer overlap

    Examples:
      | width |
      | 680   |
      | 390   |

  Scenario Outline: The Overview fits a narrow window
    When the application is observed
    And the observer's window is <width> by 900 pixels
    Then the observer has "role=heading[name=\"Overview\"]"
    And the observer's own audit finds no "layout-overlap", "text-clipped" or "layout-offscreen"

    Examples:
      | width |
      | 680   |
      | 390   |

  Scenario Outline: The Frames page fits a narrow window
    When the application is observed
    And the observer's window is <width> by 800 pixels
    And the observer's "role=tab[name=\"Frames\"]" is clicked
    Then the observer has "role=button[name=\"Record\"]"
    And every "role=button" in the observer lies within the width of its window
    And the observer's own audit finds no "layout-overlap", "text-clipped" or "layout-offscreen"

    Examples:
      | width |
      | 680   |
      | 390   |

  Scenario Outline: Every page fits a narrow window
    When the application is observed
    And the observer's window is <width> by 800 pixels
    And the observer's "role=tab[name=\"<page>\"]" is clicked
    Then every "role=button" in the observer lies within the width of its window
    And the observer's own audit finds no "layout-overlap", "text-clipped" or "layout-offscreen"

    Examples:
      | page        | width |
      | Processes   | 680   |
      | Mailboxes   | 680   |
      | Objects     | 680   |
      | Logs        | 680   |
      | Trace       | 680   |
      | Supervision | 680   |
      | Memory      | 680   |
      | Models      | 680   |
      | Wakes       | 680   |
      | Components  | 680   |
      | Bindings    | 680   |
      | Growth      | 680   |
      | Audit       | 680   |
      | Processes   | 390   |
      | Mailboxes   | 390   |
      | Objects     | 390   |
      | Logs        | 390   |
      | Trace       | 390   |
      | Supervision | 390   |
      | Memory      | 390   |
      | Models      | 390   |
      | Wakes       | 390   |
      | Components  | 390   |
      | Bindings    | 390   |
      | Growth      | 390   |
      | Audit       | 390   |
