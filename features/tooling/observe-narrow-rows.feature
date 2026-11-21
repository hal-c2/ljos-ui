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

  Scenario Outline: A wake opened leaves room for the list of wakes
    When the application is observed
    And the observer's window is <width> by 800 pixels
    And the observer's "role=tab[name=\"Wakes\"]" is clicked
    And the observer's "role=button[name=\"Record\"]" is clicked
    And the observer's "role=list[name=\"Wakes\"] >> role=listitem[name*=\"timer\"] >> nth=0" is clicked
    Then the observer has "role=button[name=\"Close\"]"
    And the observer's "role=list[name=\"Wakes\"]" is at least 300 pixels wide

    Examples:
      | width |
      | 1200  |
      | 700   |
      | 390   |
