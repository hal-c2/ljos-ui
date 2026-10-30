# Sources:
#   docs/ARCHITECTURE.md sections 5.5 (the type dump) and 19 (the authoring server)
#   https://doc.qt.io/qt-6/qtqml-syntax-signals.html
# The type dump and the authoring server are Ljós's own: Qt has nothing to compare them with.

@ljos-only @headless
Feature: Looking up a type's signals
  `type_lookup` answers what a handler may be written for and what it is
  given. A signal is written `name(argument: type, ...)`; an argument whose
  type is not known has only its name. The answer holds the type's own
  signals, and what it inherits is in the types of `base_chain`. A type of
  the project's `ljostypes.json`, which no table has, is looked up in that
  file; there a property's change signal is implied by its `notify`, and the
  answer lists it as a signal like any other.

  Background:
    Given the file "ljostypes.json":
      """json
      {"format": 2, "modules": [{"uri": "App.Media", "versions": ["1.0"], "types": [
        {"name": "Source", "since": "1.0", "base": "QtObject", "baseModule": "QtQml",
         "creatable": true, "defaultProperty": null, "attached": null, "properties": [],
         "signals": [{"name": "opened", "parameters": [{"name": "url", "type": "url"}]}],
         "methods": []},
        {"name": "Player", "since": "1.0", "base": "Source", "baseModule": "App.Media",
         "creatable": true, "defaultProperty": null, "attached": null,
         "properties": [{"name": "volume", "type": "int", "kind": "value", "readonly": false,
           "final": true, "required": false, "constant": false, "group": false,
           "notify": "volumeChanged"}],
         "signals": [{"name": "finished", "parameters": [{"name": "position", "type": "real"}]}],
         "methods": [{"name": "seek", "parameters": [{"name": "to", "type": "real"}],
           "returnType": "bool"}]}]}]}
      """
    And the authoring tools are connected

  Scenario: A type of the table says what its signals are given
    When the agent calls the tool "type_lookup" with:
      """json
      {"module": "Ljos", "type": "ErrorBoundary"}
      """
    Then the answer's "/signals/0" is "contentChanged()"

  Scenario: A type of the file lists its signals with their arguments' types
    When the agent calls the tool "type_lookup" with:
      """json
      {"module": "App.Media", "type": "Player"}
      """
    Then the answer's "/signals" is ["finished(position: real)", "volumeChanged()"]

  Scenario: What a type of the file inherits is in its base chain
    When the agent calls the tool "type_lookup" with:
      """json
      {"module": "App.Media", "type": "Player"}
      """
    Then the answer's "/base_chain" is ["Source", "QtObject"]

  Scenario: The inherited signal is found on the base
    When the agent calls the tool "type_lookup" with:
      """json
      {"module": "App.Media", "type": "Source"}
      """
    Then the answer's "/signals" is ["opened(url: url)"]

  Scenario: A module of the file lists its types
    When the agent calls the tool "type_lookup" with:
      """json
      {"module": "App.Media", "type": "Nothing"}
      """
    Then the agent is refused with "not_found"
    And the refusal mentions "Nothing"
