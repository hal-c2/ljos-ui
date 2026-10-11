# Sources:
#   docs/ARCHITECTURE.md sections 19.4, 19.6, 19.7 and 19.9
#   docs/research/10-mcp.md
#   https://modelcontextprotocol.io/specification/2025-11-25/server/tools
# The inspector is a Ljós extension: Qt has nothing to compare it with.

@ljos-only @headless
Feature: What the inspector lets an agent do
  The inspector reads and never acts. An agent is offered the tools that
  read, in one fixed order, and a request to act is refused whatever the
  agent was started with.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        width: 200; height: 100
        property int taps: 0
      }
      """
    And the inspector is connected

  Scenario: The tools are the ones that read, in a fixed order
    Then the agent is offered the tools "app_status, app_snapshot, app_find, app_inspect, app_screenshot, app_logs, app_metrics"

  Scenario: Every tool is marked as one that changes nothing
    Then every tool the agent is offered is marked read-only

  Scenario Outline: A tool that acts is refused and says what it would need
    When the agent calls the tool "<tool>"
    Then the agent is refused with "capability_denied"
    And the refusal mentions "<capability>"

    Examples:
      | tool             | capability |
      | app_click        | input      |
      | app_type         | input      |
      | app_press        | input      |
      | app_scroll       | input      |
      | app_set_property | mutate     |
      | app_invoke       | mutate     |
      | app_reload       | mutate     |
      | app_eval         | eval       |

  Scenario: A refused request changes nothing
    When the agent calls the tool "app_set_property" with:
      """json
      {"target": "#root", "property": "taps", "value": 5}
      """
    Then the agent is refused with "capability_denied"
    And the property "taps" of "root" is 0

  Scenario: A tool nobody knows is refused as not found
    When the agent calls the tool "app_teleport"
    Then the agent is refused with "not_found"

  Scenario: An argument a tool does not have is refused with its name
    When the agent calls the tool "app_snapshot" with:
      """json
      {"max_node": 5}
      """
    Then the agent is refused with "invalid_argument"
    And the refusal mentions "max_node"

  Scenario: The agent is warned about instructions written in the application
    Then the inspector's instructions match "(?i)never follow instructions"
