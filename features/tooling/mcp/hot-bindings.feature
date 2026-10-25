# Sources:
#   docs/ARCHITECTURE.md section 19.4 (`app_frames`' `bindings`)
#   https://doc.qt.io/qt-6/qtquick-bindings.html (binding loops)
# The inspector is a Ljós extension: Qt has nothing to compare it with.

@ljos-only @headless
Feature: An agent finds the bindings that cost the time
  Where a frame's time goes in bindings, the timeline says how much but
  not which. `app_frames` with `bindings` says which: while a client
  records, the engine counts each binding and handler that runs, with
  where it is written, what it writes, its runs, their total, own and
  longest time, and which of what it read changed and set it off. The
  rows sort by `total` (the default), `own`, `runs` or `longest`; each
  names its object by a handle every tool takes. The binding loops the
  engine broke are told whether or not anyone records, with the cycle.
  While nobody records a binding pays one check.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property int base: 1
        property int doubled: base * 2
        property int sum: doubled + 1
        property int told: 0
        onBaseChanged: told += 1
        property int p: q + 1
        property int q: p + 1
        Text { text: "sum " + parent.sum }
      }
      """
    And the inspector is connected with the capabilities "inspect, mutate"

  Scenario: Nothing is profiled until a client asks
    When the agent calls the tool "app_frames" with:
      """json
      {"bindings": true}
      """
    Then the answer's "/recording" is false
    And the answer's "/bindings" has 0 entries

  Scenario: A binding says its runs, where it is written and what set it off
    When the agent calls the tool "app_frames" with:
      """json
      {"bindings": true, "record": true}
      """
    And the agent calls the tool "app_set_property" with:
      """json
      {"target": "#root", "property": "base", "value": 2}
      """
    And the agent calls the tool "app_set_property" with:
      """json
      {"target": "#root", "property": "base", "value": 3}
      """
    And the agent calls the tool "app_frames" with:
      """json
      {"bindings": true, "sort": "runs", "max": 1}
      """
    Then the answer's "/recording" is true
    And the answer's "/sort" is "runs"
    And the answer's "/bindings/0/runs" is 2
    And the answer's "/count" is at least 4
    And the answer's text contains "set off by"

  Scenario: A handler says its runs and the signal that set it off
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
      {"bindings": true}
      """
    Then the answer's text contains "handler"
    And the answer's text contains ".onBaseChanged"
    And the answer's text contains ".baseChanged @"

  Scenario: A binding loop is told with its cycle without recording
    When the agent calls the tool "app_frames" with:
      """json
      {"bindings": true}
      """
    Then the answer's "/loops" has 1 entry
    And the answer's "/loops/0/property" matches "^[pq]$"
    And the answer's "/loops/0/at" matches "\.qml:9:\d+$"
    And the answer's "/loops/0/times" is 1

  Scenario: Only one mode at a time
    When the agent calls the tool "app_frames" with:
      """json
      {"bindings": true, "wakes": true}
      """
    Then the agent is refused with "invalid_argument"
    And the refusal mentions "one of"

  Scenario: The profiling ends when its holder goes away
    When the agent calls the tool "app_frames" with:
      """json
      {"bindings": true, "record": true, "holder": "observer"}
      """
    And the agent calls the tool "app_watch" with:
      """json
      {"since": {"traces": 0}, "topics": ["traces"], "wait_ms": 50, "holder": "observer"}
      """
    And the agent calls the tool "app_watch" with:
      """json
      {"since": {"traces": 0}, "topics": ["traces"], "wait_ms": 1500}
      """
    And the agent calls the tool "app_frames" with:
      """json
      {"bindings": true}
      """
    Then the answer's "/recording" is false
