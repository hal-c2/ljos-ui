# Sources:
#   docs/ARCHITECTURE.md (section 19.5, `ljos observe`)
# The observer is a Ljós tool: Qt has nothing to compare it with.

@ljos-only @desktop
Feature: What the observer lists stays readable in a narrow window
  A page of the observer lists rows: a log line, a change it traced, a
  component and how much it holds. In a narrow window a row too long for
  one line takes two, so that every word in it is inside the window and
  none is elided to nothing. A page that holds more than one list keeps
  room for each, so that opening one thing never squeezes another away.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import Ljos
      Item {
        width: 200; height: 100
        Item {
          id: ticker
          objectName: "ticker"
          property int ticks: 0
          Timer { interval: 50; running: true; repeat: true; onTriggered: ticker.ticks += 1 }
        }
        ListView {
          width: 100; height: 40
          model: ListModel {
            objectName: "animals"
            ListElement { ant1: 1; bee2: 2; cat3: 3; dog4: 4; zebra5: 5 }
          }
          delegate: Item { width: 100; height: 20 }
        }
        Repeater {
          model: 8
          ErrorBoundary { Item {} }
        }
        Component.onCompleted: console.warn("a line the application logged")
      }
      """

  Scenario Outline: A log line shows all its words
    When the application is observed
    And the observer's window is <width> by 800 pixels
    And the observer's "role=tab[name=\"Logs\"]" is clicked
    Then the observer shows "a line the application logged" whole

    Examples:
      | width |
      | 680   |
      | 390   |

  Scenario Outline: A traced change shows what changed
    When the application is observed
    And the observer's window is <width> by 800 pixels
    And the observer's "role=tab[name=\"Objects\"]" is clicked
    And the observer's "role=listitem[name*=\"ticker\"]" is clicked
    And the observer's "role=button[name=\"Trace\"]" is clicked
    And the observer's "role=tab[name=\"Trace\"]" is clicked
    Then the observer shows "ticks" whole

    Examples:
      | width |
      | 680   |
      | 390   |

  Scenario Outline: A component's row shows its counts
    When the application is observed
    And the observer's window is <width> by 800 pixels
    And the observer's "role=tab[name=\"Components\"]" is clicked
    Then the observer has "role=list[name=\"Components\"] >> role=listitem"
    And every "role=list[name=\"Components\"] >> role=label" in the observer lies within the width of its window

    Examples:
      | width |
      | 680   |
      | 390   |

  Scenario: What each window keeps is inside the window
    When the application is observed
    And the observer's window is 390 by 800 pixels
    And the observer's "role=tab[name=\"Memory\"]" is clicked
    Then the observer has "text=renderer: "
    And every "name~=^(scene|renderer): " in the observer lies within the width of its window

  # Twelve supervisors: the application's four and its eight boundaries.
  Scenario: Many supervisors leave room for the faults
    When the application is observed
    And the observer's window is 390 by 800 pixels
    And the observer's "role=tab[name=\"Supervision\"]" is clicked
    Then the observer has "role=listitem[name*=\"ErrorBoundary\"]"
    And the observer's "role=list[name=\"Faults\"]" is at least 120 pixels high

  # A window shorter than the supervisors and the room the faults keep:
  # the page scrolls to the faults rather than run past its bottom.
  Scenario: A window too short for the supervisors and the faults scrolls to the faults
    When the application is observed
    And the observer's window is 390 by 480 pixels
    And the observer's "role=tab[name=\"Supervision\"]" is clicked
    And the observer's "role=listitem[name*=\"ErrorBoundary\"]" is shown
    And the observer's "role=heading[name=\"Supervision\"]" is scrolled 20 notches down
    Then the observer's "role=list[name=\"Faults\"]" is at least 120 pixels high
    And the observer shows "No faults" whole

  # The ticker wakes the application every 50 ms. The list of wakes
  # follows the newest, and only the newest keep what they did, so the
  # newest timer wake is the one opened.
  Scenario Outline: A wake opened leaves room for the list of wakes
    When the application is observed
    And the observer's window is <width> by 800 pixels
    And the observer's "role=tab[name=\"Wakes\"]" is clicked
    And the observer's "role=button[name=\"Record\"]" is clicked
    And the observer's "role=list[name=\"Wakes\"] >> role=listitem[name*=\"timer\"] >> nth=-1" is clicked
    Then the observer has "role=button[name=\"Close\"]"
    And the observer's "role=list[name=\"Wakes\"]" is at least 300 pixels wide

    Examples:
      | width |
      | 1200  |
      | 700   |
      | 390   |

  # Below the width where the value of a change has room beside the
  # columns before it, the value goes on a line of its own: one line with
  # the value squeezed to a sliver is never chosen.
  Scenario Outline: A traced change keeps room for its value
    When the application is observed
    And the observer's window is <width> by 800 pixels
    And the observer's "role=tab[name=\"Objects\"]" is clicked
    And the observer's "role=listitem[name*=\"ticker\"]" is clicked
    And the observer's "role=button[name=\"Trace\"]" is clicked
    And the observer's "role=tab[name=\"Trace\"]" is clicked
    Then the observer has "role=list[name=\"Trace\"] >> name~=\"^[0-9]+$\""
    And the observer's "role=list[name=\"Trace\"] >> name~=\"^[0-9]+$\"" is at least 80 pixels wide

    Examples:
      | width |
      | 640   |
      | 700   |
      | 390   |

  # Where a row of frames is one line, the bar gets what is left after the
  # cells around it, so what each frame ran is never cut off at the card.
  Scenario Outline: A recorded frame's row is inside its card
    When the application is observed
    And the observer's window is <width> by 800 pixels
    And the observer's "role=tab[name=\"Frames\"]" is clicked
    And the observer's "role=button[name=\"Record\"]" is clicked
    Then the observer has "name~=\"^frame [0-9]+ of \""
    And the observer's own audit finds no "layout-overlap", "text-clipped" or "layout-offscreen"

    Examples:
      | width |
      | 720   |
      | 740   |
      | 760   |

  # Five roles at least 90 pixels each are wider than a phone: the roles
  # and their rows scroll sideways together, so the last is reached.
  Scenario: A model's last role is reached in a narrow window
    When the application is observed
    And the observer's window is 390 by 800 pixels
    And the observer's "role=tab[name=\"Models\"]" is clicked
    And the observer's "role=listitem[name*=\"animals\"]" is clicked
    And the observer's "name=\"ant1\"" is scrolled 20 notches right
    Then the observer shows "zebra5" whole

  # The rows' own scroll bar stays by the part of the table in view, not
  # at the far edge of columns scrolled out of it.
  Scenario: A model's rows scroll bar is in view in a narrow window
    When the application is observed
    And the observer's window is 390 by 800 pixels
    And the observer's "role=tab[name=\"Models\"]" is clicked
    And the observer's "role=listitem[name*=\"animals\"]" is clicked
    Then every "role=scrollbar[name=\"Rows\"]" in the observer lies within the width of its window

  # Where the table scrolls both ways, the rows' bar stops above the
  # columns' bar rather than cover the corner where the two meet.
  Scenario: A model's two scroll bars do not cover each other in a narrow window
    When the application is observed
    And the observer's window is 390 by 800 pixels
    And the observer's "role=tab[name=\"Models\"]" is clicked
    And the observer's "role=listitem[name*=\"animals\"]" is clicked
    Then the observer has "role=scrollbar[name=\"Rows\"]"
    And no two "role=scrollbar" in the observer overlap

  # Where the table fits its width there is no sideways bar to stop above:
  # the rows' bar runs the length of the rows.
  Scenario: A model's rows scroll bar is as long as its rows in a wide window
    When the application is observed
    And the observer's window is 1200 by 800 pixels
    And the observer's "role=tab[name=\"Models\"]" is clicked
    And the observer's "role=listitem[name*=\"animals\"]" is clicked
    Then the observer's "role=scrollbar[name=\"Rows\"]" ends level with its "role=list[name=\"Rows\"]"

  # Stacked in a short window, the list, the wake opened and why the UI
  # thread is awake need more height than there is: the page scrolls to
  # the last of them rather than squeeze it away.
  Scenario: A wake opened in a short narrow window scrolls to why it is awake
    When the application is observed
    And the observer's window is 390 by 520 pixels
    And the observer's "role=tab[name=\"Wakes\"]" is clicked
    And the observer's "role=button[name=\"Record\"]" is clicked
    And the observer's "role=list[name=\"Wakes\"] >> role=listitem[name*=\"timer\"] >> nth=-1" is clicked
    And the observer's "role=button[name=\"Close\"]" is scrolled 20 notches down
    Then the observer's "role=list[name=\"Wakes\"]" is at least 120 pixels high
    And the observer shows "Why awake" whole
