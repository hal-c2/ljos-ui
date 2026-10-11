# Sources:
#   docs/ARCHITECTURE.md sections 5.4 (binding tiers) and 19.3
#   https://doc.qt.io/qt-6/qml-qtquick-propertychanges.html
# The authoring tools are a Ljós extension: Qt has nothing to compare them with.
# A file given here is written without the spaces its doc string is indented by, so the
# first column of a line is 1.

@ljos-only @headless
Feature: The tier of a value a state gives
  What a `PropertyChanges` gives a property is a binding like any other: one
  that only reads a property costs almost nothing, arithmetic runs in the
  binding VM and the rest is handed to JavaScript. It is found, explained
  and counted with the bindings of the file.

  Background:
    Given the authoring tools are connected
    And the file "Main.qml":
      """qml
      import QtQuick

      Item {
          id: root
          property int n: 2
          states: State {
              name: "big"
              PropertyChanges {
                  target: root
                  width: root.n
                  height: n * 10
                  opacity: Math.random() * n
                  z: 3
              }
              PropertyChanges { root.x: n + 1 }
          }
      }
      """

  Scenario: A state's value that only reads a property is tier 0
    When the agent calls the tool "binding_tier_explain" with:
      """json
      {"file": "Main.qml", "line": 10}
      """
    Then the answer's "/expressions/0/property" is "width"
    And the answer's "/expressions/0/object" is "PropertyChanges"
    And the answer's "/expressions/0/kind" is "binding"
    And the answer's "/expressions/0/tier" is "0"

  Scenario: Arithmetic in a state's value runs in the binding VM
    When the agent calls the tool "binding_tier_explain" with:
      """json
      {"file": "Main.qml", "line": 11}
      """
    Then the answer's "/expressions/0/tier" is "1"
    And the answer's "/expressions/0/source" is "n * 10"

  Scenario: The rest is left to JavaScript, with the reason
    When the agent calls the tool "binding_tier_explain" with:
      """json
      {"file": "Main.qml", "line": 12}
      """
    Then the answer's "/expressions/0/tier" is "js"
    And the answer's "/expressions/0/reason/kind" is "global function call"
    And the answer's "/expressions/0/reason/detail" is "Math.random"

  Scenario: A value that is written out is not an expression
    When the agent calls the tool "binding_tier_explain" with:
      """json
      {"file": "Main.qml", "line": 13}
      """
    Then the answer's "/expressions/0/kind" is "assigned"
    And the answer's "/expressions/0/tier" is null

  Scenario: A value written with the target's name in front has a tier too
    When the agent calls the tool "binding_tier_explain" with:
      """json
      {"file": "Main.qml", "line": 15}
      """
    Then the answer's "/expressions/0/tier" is "1"
    And the answer's "/expressions/0/source" is "n + 1"

  Scenario: The values of a state are counted with the bindings of the file
    When the agent calls the tool "binding_tier_explain" with:
      """json
      {"file": "Main.qml"}
      """
    Then the answer's "/summary/expressions" is 5
    And the answer's "/summary/tier0" is 2
    And the answer's "/summary/tier1" is 2
    And the answer's "/summary/js" is 1
