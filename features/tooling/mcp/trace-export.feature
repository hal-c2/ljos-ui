# Sources:
#   docs/ARCHITECTURE.md section 19.4 (`app_frames`' `export`)
#   https://docs.google.com/document/d/1CvAClvFfyA5R-PhYUmn5OOQtYMH4h6I0nSsKchNAySU
#   (the Trace Event Format that Perfetto and chrome://tracing read)
# The inspector is a Ljós extension: Qt has nothing to compare it with.

@ljos-only @headless
Feature: An agent keeps a recording as a trace file
  What `app_frames` recorded (the frames with their passes, the wakes,
  and the runs of bindings and handlers) can be kept as a trace file a
  standard trace viewer opens. `export` names the file; the application
  writes it, whole or not at all, in the folder its host gave for trace
  files (beside its token file, `target/ljos/traces/`), and the answer
  says where and how much it holds. A client names a file, never a
  place: a name with a folder in it is refused.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property int base: 1
        property int sum: base + 1
        Text { text: "sum " + parent.sum }
      }
      """
    And the inspector is connected with the capabilities "inspect, mutate"

  Scenario: The bindings recorded are written to a trace file
    When the agent calls the tool "app_frames" with:
      """json
      {"bindings": true, "record": true}
      """
    And the agent calls the tool "app_set_property" with:
      """json
      {"target": "#root", "property": "base", "value": 2}
      """
    And the agent calls the tool "app_frames" with:
      """json
      {"export": "run-1"}
      """
    Then the answer's "/path" matches "/run-1\.json$"
    And the answer's "/runs" is at least 2
    And the answer's text contains "open it in a trace viewer"

  Scenario: Nothing recorded is nothing to write
    When the agent calls the tool "app_frames" with:
      """json
      {"export": "run-1"}
      """
    Then the agent is refused with "invalid_argument"
    And the refusal mentions "nothing is recorded"

  Scenario: A name with a folder in it is refused
    When the agent calls the tool "app_frames" with:
      """json
      {"export": "../run-1"}
      """
    Then the agent is refused with "invalid_argument"
    And the refusal mentions "not a path"
