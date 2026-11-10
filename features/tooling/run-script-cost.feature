# Sources:
#   docs/ARCHITECTURE.md sections 5.4 (binding tiers), 12.1 (development) and 19.4 (`app_frames`)
#   docs/script-cost.md (the hint and the order its cheaper forms are tried in)
#   docs/declarative.md (the `decl-*` forms)
# `ljos run` and its inspector are Ljós's own: Qt has nothing to compare them with.

@ljos-only @headless
Feature: The cheaper form `ljos run` gives a script that costs
  `ljos run` links the authoring tools, so the script-cost hints of its
  inspector (`app_frames` with `script`) name the cheapest form they know.
  First the rewrite `binding_tier_explain` offers, checked against the
  compiler, then the declarative form a `decl-*` lint of `qml_lint` names
  on the same line, and only when neither is known moving the work into
  Rust: a native type, or compiling the files the application ships ahead
  of time. Finding the form reads and compiles the file away from the
  application, so asking never holds up what the application draws.

  Scenario: A binding with a rewrite is told the rewrite
    Given the file "Main.qml":
      """qml
      import QtQuick
      Item {
          objectName: "root"
          property int count: 3
          scale: (function() { return count / 2 })()
      }
      """
    When the author starts "ljos run Main.qml --headless --mcp=inspect,mutate"
    And the agent calls the tool "app_frames" with:
      """json
      {"script": true, "record": true}
      """
    And the agent calls the tool "app_set_property" with:
      """json
      {"target": "#root", "property": "count", "value": 4}
      """
    And the agent calls the tool "app_frames" with:
      """json
      {"script": true, "min_run_ms": 0}
      """
    Then the answer's "/hints/0/name" is "scale"
    And the answer's "/hints/0/cheaper/form" is "rewrite"
    And the answer's "/hints/0/cheaper/source" is "count / 2"
    And the answer's "/hints/0/cheaper/tier" is "1"
    And the answer's text contains "count / 2"

  Scenario: A handler a declaration says better is told the declaration
    Given the file "Main.qml":
      """qml
      import QtQuick
      Item {
          objectName: "root"
          property real level: 0
          property string shown: ""
          onLevelChanged: shown = level.toString(16)
      }
      """
    When the author starts "ljos run Main.qml --headless --mcp=inspect,mutate"
    And the agent calls the tool "app_frames" with:
      """json
      {"script": true, "record": true}
      """
    And the agent calls the tool "app_set_property" with:
      """json
      {"target": "#root", "property": "level", "value": 2}
      """
    And the agent calls the tool "app_frames" with:
      """json
      {"script": true, "min_run_ms": 0}
      """
    Then the answer's "/hints/0/name" is "onLevelChanged"
    And the answer's "/hints/0/cheaper/form" is "declarative"
    And the answer's "/hints/0/cheaper/code" is "decl-mirror-handler"
    And the answer's "/hints/0/cheaper/guide" is "docs/declarative.md#decl-mirror-handler"

  Scenario: Script with no cheaper form in QML is told to move into Rust, and why
    Given the file "Main.qml":
      """qml
      import QtQuick
      Item {
          id: root
          objectName: "root"
          property int count: 3
          property string label: ["a", "b"].join(count)
          Text { text: root.label }
      }
      """
    When the author starts "ljos run Main.qml --headless --mcp=inspect,mutate"
    And the agent calls the tool "app_frames" with:
      """json
      {"script": true, "record": true}
      """
    And the agent calls the tool "app_set_property" with:
      """json
      {"target": "#root", "property": "count", "value": 4}
      """
    And the agent calls the tool "app_frames" with:
      """json
      {"script": true, "min_run_ms": 0}
      """
    Then the answer's "/hints/0/name" is "label"
    And the answer's "/hints/0/cheaper/form" is "rust"
    And the answer's "/hints/0/cheaper/text" matches "native type"
    And the answer's "/hints/0/cheaper/reason" matches "method call"
