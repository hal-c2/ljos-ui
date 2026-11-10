# Sources:
#   docs/ARCHITECTURE.md sections 5.4 (binding tiers), 5.5 (native types) and 19.4 (`app_frames`)
#   docs/script-cost.md (the hint, its threshold and the cheaper forms)
#   docs/declarative.md (the `decl-*` forms a hint can point to)
# The inspector is a Ljós extension: Qt has nothing to compare it with.

@ljos-only @headless
Feature: An agent is told where script time costs and what is cheaper
  Script is never forbidden. A binding or handler the binding VM cannot
  run is left to JavaScript, which is correct and only slower, and most of
  the time it does not matter. `app_frames` with `script` says where it
  does: while a client records (the recording `bindings` reads), the
  engine counts the time each binding and handler spends in its own
  script, less what it set off that was measured on its own. One is hinted
  when a single run spent at least 2 milliseconds in script, or when its
  script took at least a tenth of the time every binding and handler took
  together and at least 1 millisecond in all. Each hint is a hint, never
  an error: it says where the binding or handler is written, what its
  script cost and a cheaper form, which is the verified rewrite when the
  authoring tools know one, else the declarative form a `decl-*` lint
  names, else moving the work into Rust. Nothing is measured until a
  client records, and the recording asks for no work of its own.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property int base: 1
        property real heavy: { var s = 0; for (var i = 0; i < 400000; i++) s += i % 7; return s + base }
        property string label: ["a", "b"].join(base)
        property int cheap: base + 1
        property int other: 1
        property real sum: 0
        onOtherChanged: { var s = 0; for (var i = 0; i < 400000; i++) s += i % 7; sum = s + other }
        Text { text: root.heavy + " " + root.label + " " + root.cheap }
      }
      """
    And the inspector is connected with the capabilities "inspect, mutate"

  Rule: Script time is measured only while a client records

    Scenario: Nothing is measured until a client asks
      When the agent calls the tool "app_frames" with:
        """json
        {"script": true}
        """
      Then the answer's "/recording" is false
      And the answer's "/hints" has 0 entries
      And the answer's text contains "record"

    Scenario: The recording asks for no work of its own
      When the agent calls the tool "app_frames" with:
        """json
        {"script": true, "record": true}
        """
      Then the application is left idle

    Scenario: The script time reads the recording `bindings` starts
      When the agent calls the tool "app_frames" with:
        """json
        {"bindings": true, "record": true}
        """
      And the agent calls the tool "app_frames" with:
        """json
        {"script": true}
        """
      Then the answer's "/recording" is true

    Scenario: The measuring ends when its holder goes away
      When the agent calls the tool "app_frames" with:
        """json
        {"script": true, "record": true, "holder": "observer"}
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
        {"script": true}
        """
      Then the answer's "/recording" is false

  Rule: A binding or handler whose script costs is hinted with where it is written and what it cost

    Scenario: A run that spends long in script is hinted
      When the agent calls the tool "app_frames" with:
        """json
        {"script": true, "record": true}
        """
      And the agent calls the tool "app_set_property" with:
        """json
        {"target": "#root", "property": "base", "value": 2}
        """
      And the agent calls the tool "app_frames" with:
        """json
        {"script": true}
        """
      Then the answer's "/hints" has 1 entry
      And the answer's "/hints/0/code" is "script-cost"
      And the answer's "/hints/0/severity" is "hint"
      And the answer's "/hints/0/kind" is "binding"
      And the answer's "/hints/0/name" is "heavy"
      And the answer's "/hints/0/at" matches "Scenario\.qml:5:\d+$"
      And the answer's "/hints/0/line" is 5
      And the answer's "/hints/0/runs" is 1
      And the answer's "/hints/0/longest_script_us" is at least 2000
      And the answer's "/hints/0/criteria/0" is "run"
      And the answer's "/hints/0/guide" is "docs/script-cost.md"
      And the answer's text contains "hint script-cost"

    Scenario: A share of all the time is hinted though no run is long
      When the agent calls the tool "app_frames" with:
        """json
        {"script": true, "record": true}
        """
      And the agent calls the tool "app_set_property" with:
        """json
        {"target": "#root", "property": "base", "value": 2}
        """
      And the agent calls the tool "app_frames" with:
        """json
        {"script": true, "min_run_ms": 10000}
        """
      Then the answer's "/hints" has 1 entry
      And the answer's "/hints/0/name" is "heavy"
      And the answer's "/hints/0/criteria" is ["share"]
      And the answer's "/hints/0/share_percent" is at least 10

    Scenario: A lower threshold hints cheap script too, and never what runs without script
      When the agent calls the tool "app_frames" with:
        """json
        {"script": true, "record": true}
        """
      And the agent calls the tool "app_set_property" with:
        """json
        {"target": "#root", "property": "base", "value": 2}
        """
      And the agent calls the tool "app_frames" with:
        """json
        {"script": true, "min_run_ms": 0}
        """
      Then the answer's "/hints" has 2 entries
      And the answer's "/hints/0/name" is "heavy"
      And the answer's "/hints/1/name" is "label"

    Scenario: A handler whose script costs is hinted by its name
      When the agent calls the tool "app_frames" with:
        """json
        {"script": true, "record": true}
        """
      And the agent calls the tool "app_set_property" with:
        """json
        {"target": "#root", "property": "other", "value": 2}
        """
      And the agent calls the tool "app_frames" with:
        """json
        {"script": true}
        """
      Then the answer's "/hints" has 1 entry
      And the answer's "/hints/0/kind" is "handler"
      And the answer's "/hints/0/name" is "onOtherChanged"
      And the answer's "/hints/0/line" is 10

    Scenario: Without the authoring tools the cheaper form is moving the work into Rust
      When the agent calls the tool "app_frames" with:
        """json
        {"script": true, "record": true}
        """
      And the agent calls the tool "app_set_property" with:
        """json
        {"target": "#root", "property": "base", "value": 2}
        """
      And the agent calls the tool "app_frames" with:
        """json
        {"script": true}
        """
      Then the answer's "/hints/0/cheaper/form" is "rust"
      And the answer's "/hints/0/cheaper/text" matches "native type"

  Rule: The thresholds an agent gives are checked

    Scenario Outline: A threshold that means nothing is refused
      When the agent calls the tool "app_frames" with:
        """json
        <args>
        """
      Then the agent is refused with "invalid_argument"
      And the refusal mentions "<says>"

      Examples:
        | args                                 | says             |
        | {"script": true, "min_run_ms": -1}   | min_run_ms       |
        | {"script": true, "min_run_ms": 1e30} | too large        |
        | {"script": true, "min_share": 101}   | min_share        |
        | {"bindings": true, "min_run_ms": 1}  | go with `script` |
        | {"script": true, "bindings": true}   | one of           |
