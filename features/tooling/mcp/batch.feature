# Sources:
#   docs/ARCHITECTURE.md sections 19.4 and 19.6
#   docs/research/10-mcp.md
# The inspector is a Ljós extension: Qt has nothing to compare it with.

@ljos-only @headless
Feature: Several steps in one call
  An agent that knows what it will do next can say so in one call: the steps
  run one after the other, the first that fails stops the rest, and what
  changed is told once, at the end, as one difference between the screen
  before the first step and the screen after the last. Each step says what
  it cost.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        id: top
        objectName: "root"
        width: 300; height: 200
        property int clicks: 0
        Rectangle {
          objectName: "save"
          x: 10; y: 10; width: 80; height: 30
          Accessible.role: Accessible.Button
          Accessible.name: top.clicks > 0 ? "Saved" : "Save"
          Text { anchors.centerIn: parent; text: top.clicks > 0 ? "Saved" : "Save" }
          MouseArea { anchors.fill: parent; onClicked: top.clicks += 1 }
        }
        TextInput {
          objectName: "field"
          x: 100; y: 10; width: 120; height: 20
          focus: true
          Accessible.name: "Field"
        }
      }
      """
    And the inspector is connected with the capabilities "inspect, input"
    And the agent has requested a snapshot

  Rule: The steps run in order

    Scenario: Steps run one after the other
      When the agent calls the tool "app_batch" with:
        """json
        {"steps": [
          {"tool": "app_click", "args": {"target": "#save"}},
          {"tool": "app_type", "args": {"text": "hi"}},
          {"tool": "app_press", "args": {"keys": "Backspace"}}
        ]}
        """
      Then the property "clicks" of "root" is 1
      And the property "text" of "field" is "h"
      And the answer's "/completed" is 3
      And the answer's "/failed" is null

    Scenario: A step is told by the answer it would have given alone
      When the agent calls the tool "app_batch" with:
        """json
        {"steps": [{"tool": "app_type", "args": {"text": "hi"}}]}
        """
      Then the answer's "/steps/0/tool" is "app_type"
      And the answer's "/steps/0/ok" is true
      And the answer's "/steps/0/result/typed" is "hi"

    Scenario: Each step says what it cost
      When the agent calls the tool "app_batch" with:
        """json
        {"steps": [{"tool": "app_click", "args": {"target": "#save"}}]}
        """
      Then the answer's "/steps/0/cost/frames" is at least 1
      And the answer's "/steps/0/cost/ms" is at least 0

  Rule: The first step that fails stops the rest

    Scenario: A step that is refused stops the batch and is named
      When the agent calls the tool "app_batch" with:
        """json
        {"steps": [
          {"tool": "app_click", "args": {"target": "#save"}},
          {"tool": "app_click", "args": {"target": "#nowhere"}},
          {"tool": "app_click", "args": {"target": "#save"}}
        ]}
        """
      Then the answer's "/completed" is 1
      And the answer's "/failed/step" is 1
      And the answer's "/failed/code" is "not_found"
      And the answer's "/steps/1/ok" is false
      And the property "clicks" of "root" is 1

    Scenario: The steps that did not run are not in the answer
      When the agent calls the tool "app_batch" with:
        """json
        {"steps": [
          {"tool": "app_click", "args": {"target": "#nowhere"}},
          {"tool": "app_click", "args": {"target": "#save"}}
        ]}
        """
      Then the answer's "/steps" has 1 entries

    Scenario: A step that does not exist is refused before anything is done
      When the agent calls the tool "app_batch" with:
        """json
        {"steps": [
          {"tool": "app_click", "args": {"target": "#save"}},
          {"tool": "app_teleport"}
        ]}
        """
      Then the agent is refused with "invalid_argument"
      And the refusal mentions "app_teleport"
      And the property "clicks" of "root" is 0

    Scenario: A step that only reads is no step of a batch
      When the agent calls the tool "app_batch" with:
        """json
        {"steps": [{"tool": "app_snapshot"}]}
        """
      Then the agent is refused with "invalid_argument"
      And the refusal mentions "app_snapshot"

    Scenario: A batch cannot hold a batch
      When the agent calls the tool "app_batch" with:
        """json
        {"steps": [{"tool": "app_batch", "args": {"steps": []}}]}
        """
      Then the agent is refused with "invalid_argument"
      And the refusal mentions "app_batch"

    Scenario: A batch of nothing is refused
      When the agent calls the tool "app_batch" with:
        """json
        {"steps": []}
        """
      Then the agent is refused with "invalid_argument"
      And the refusal mentions "steps"

    Scenario: A batch is cut at 32 steps
      When the agent calls the tool "app_batch" with 33 steps of "app_press"
      Then the agent is refused with "invalid_argument"
      And the refusal mentions "32"

  Rule: What changed is told once

    Scenario: The difference is from before the first step to after the last
      When the agent calls the tool "app_batch" with:
        """json
        {"steps": [
          {"tool": "app_click", "args": {"target": "#save"}},
          {"tool": "app_type", "args": {"text": "hi"}}
        ]}
        """
      Then the answer's "/epoch" is 2
      And the answer's "/delta" matches "Saved"
      And the answer's "/settled" is true

    Scenario: A step does not carry a difference of its own
      When the agent calls the tool "app_batch" with:
        """json
        {"steps": [{"tool": "app_click", "args": {"target": "#save"}}]}
        """
      Then the answer's "/steps/0/result/delta" is absent

    Scenario: A step that stops the batch still says what changed so far
      When the agent calls the tool "app_batch" with:
        """json
        {"steps": [
          {"tool": "app_click", "args": {"target": "#save"}},
          {"tool": "app_click", "args": {"target": "#nowhere"}}
        ]}
        """
      Then the answer's "/delta" matches "Saved"
