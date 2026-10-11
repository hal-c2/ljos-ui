# Sources:
#   docs/ARCHITECTURE.md sections 19.4 and 19.9
#   docs/research/10-mcp.md
#   https://doc.qt.io/qt-6/qml-qtquick-accessible.html
# The inspector is a Ljós extension: Qt has nothing to compare it with.

@ljos-only @headless
Feature: Finding items with the inspector
  An agent that knows what it is looking for asks for it by role, name,
  object name, text or state, which costs less than a whole snapshot.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        width: 300; height: 300
        Item {
          objectName: "tools"
          width: 300; height: 40
          Accessible.role: Accessible.ToolBar
          Accessible.name: "Tools"
          Rectangle {
            objectName: "delete"; width: 40; height: 40
            Accessible.role: Accessible.Button; Accessible.name: "Delete"
          }
        }
        Rectangle {
          y: 50; width: 40; height: 40; enabled: false
          Accessible.role: Accessible.Button; Accessible.name: "Delete"
        }
        Rectangle {
          y: 100; width: 40; height: 40
          Accessible.role: Accessible.Button; Accessible.name: "Delete all"
        }
        Text { y: 150; text: "3 notes"; Accessible.role: Accessible.StaticText }
      }
      """
    And the inspector is connected

  Scenario Outline: A locator finds the items it describes
    When the agent searches for "<locator>"
    Then the search finds <count> nodes

    Examples:
      | locator                                  | count |
      | role=button                              | 3     |
      | role=button[name="Delete"]               | 2     |
      | role=button[name*="all"]                 | 1     |
      | role=button[name~="^Delete( all)?$"]     | 3     |
      | role=button[disabled]                    | 1     |
      | role=toolbar                             | 1     |
      | #delete                                  | 1     |
      | text=notes                               | 1     |
      | text=DELETE                              | 3     |
      | name="Delete"                            | 2     |
      | name~=all$                               | 1     |
      | role=toolbar >> role=button              | 1     |
      | role=toolbar >> text=all                 | 0     |
      | role=button[name="Delete"] >> nth=1      | 1     |
      | role=checkbox                            | 0     |

  Scenario: What is found is listed with a reference and where it is
    When the agent searches for "role=toolbar >> role=button"
    Then the search finds a "button" named "Delete"
    And the answer's "/matches/0/ref" matches "^e\d+$"
    And the answer's "/matches/0/path" is "window > toolbar \"Tools\""
    And the answer's "/matches/0/obj" is "delete"
    And the answer's "/matches/0/bounds" is [0, 0, 40, 40]

  Scenario: The second of two items with one name is the one further down
    When the agent searches for "role=button[name=\"Delete\"] >> nth=1"
    Then the answer's "/matches/0/states" is ["disabled"]

  Scenario: A reference from a search can be used like one from a snapshot
    Given the agent has searched for "#delete"
    When the agent inspects the reference of the "button" named "Delete"
    Then the answer's "/object_name" is "delete"

  Scenario: A locator nobody can read is refused with the reason
    When the agent searches for "role=button[name"
    Then the agent is refused with "invalid_argument"
    And the refusal mentions "locator"

  Scenario: A pattern that is not a regular expression is refused
    When the agent searches for "name~=("
    Then the agent is refused with "invalid_argument"

  Scenario: A request for one item that fits two is refused with the candidates
    When the agent inspects "role=button[name=\"Delete\"]"
    Then the agent is refused with "ambiguous"
    And the refusal names 2 candidates
    And the refusal mentions "nth="
