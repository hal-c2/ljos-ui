# Sources:
#   docs/ARCHITECTURE.md sections 12.1, 19.4, 19.6 and 19.7
#   docs/research/10-mcp.md
#   https://doc.qt.io/qt-6/qtqml-syntax-propertybinding.html
#   https://doc.qt.io/qt-6/qml-qtqml-qtobject.html
#   https://doc.qt.io/qt-6/qtqml-syntax-signals.html
# The inspector is a Ljós extension: Qt has nothing to compare it with.

@ljos-only @headless
Feature: An agent that may change the application
  With the capability `mutate` an agent can assign a property, call a
  method or send a signal of an object, and ask for a reload. A property
  assigned this way is assigned the way a script assigns it, so it replaces
  a binding, and the inspector says that the assignment was the agent's.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: top
        objectName: "root"
        width: 200; height: 100
        property int count: 0
        property string title: "first"
        property bool on: false
        property var list: []
        property var chosen: ""
        property int twice: count * 2
        readonly property int fixed: 7
        signal pinged(string who)
        onPinged: (who) => top.chosen = who
        function add(a, b) { return a + b }
        function bump() { count += 1 }
        function fail() { throw new Error("no good") }
        function drop() { children[0].destroy() }
        Rectangle { objectName: "save"; width: 80; height: 30; Accessible.role: Accessible.Button; Accessible.name: "Save" }
        TextInput { objectName: "secret"; y: 40; width: 100; height: 20; echoMode: TextInput.Password; Accessible.name: "Secret" }
      }
      """
    And the inspector is connected with the capabilities "inspect, mutate"
    And the agent has requested a snapshot

  Rule: A property is assigned as a script assigns it

    Scenario Outline: A value of each kind is assigned
      When the agent calls the tool "app_set_property" with:
        """json
        {"target": "#root", "property": "<property>", "value": <value>}
        """
      Then the property "<property>" of "root" is <value>

      Examples:
        | property | value              |
        | count    | 5                  |
        | title    | "second"           |
        | on       | true               |
        | list     | [1, 2, 3]          |

    Scenario: The answer says what the property holds now
      When the agent calls the tool "app_set_property" with:
        """json
        {"target": "#root", "property": "count", "value": 5}
        """
      Then the answer's "/property" is "count"
      And the answer's "/value" is 5
      And the answer's "/previous" is 0

    Scenario: A property is assigned through a reference
      When the agent calls the tool "app_set_property" on the reference of the "button" named "Save" with:
        """json
        {"property": "width", "value": 120}
        """
      Then the property "width" of "save" is 120

    Scenario: An assignment replaces the binding that was there
      When the agent calls the tool "app_set_property" with:
        """json
        {"target": "#root", "property": "twice", "value": 99}
        """
      And the property "count" of "root" is set to 4
      Then the property "twice" of "root" is 99

    Scenario: An assignment is recorded as the agent's
      When the agent calls the tool "app_set_property" with:
        """json
        {"target": "#root", "property": "title", "value": "second"}
        """
      And the agent inspects "#root"
      Then the inspected property "title" is "assigned"
      And the inspected property "title" is assigned by "devtools"

    Scenario: A property nobody assigned is not recorded as the agent's
      When the agent inspects "#root"
      Then the inspected property "count" is not assigned by "devtools"

    Scenario: A property that does not exist is refused with its name
      When the agent calls the tool "app_set_property" with:
        """json
        {"target": "#root", "property": "cuont", "value": 1}
        """
      Then the agent is refused with "not_found"
      And the refusal mentions "cuont"

    Scenario: A property that cannot be assigned is refused
      When the agent calls the tool "app_set_property" with:
        """json
        {"target": "#root", "property": "fixed", "value": 1}
        """
      Then the agent is refused with "not_actionable"
      And the refusal mentions "read-only"
      And the property "fixed" of "root" is 7

    Scenario: A value of the wrong kind is refused and nothing changes
      When the agent calls the tool "app_set_property" with:
        """json
        {"target": "#root", "property": "count", "value": {"deep": true}}
        """
      Then the agent is refused with "invalid_argument"
      And the property "count" of "root" is 0

    Scenario: A reference to an item that is gone is stale
      When the method "drop" of "root" is called
      And the agent calls the tool "app_set_property" on the reference of the "button" named "Save" with:
        """json
        {"property": "width", "value": 1}
        """
      Then the agent is told the reference is stale

    Scenario: What is assigned to a field that hides its text is not echoed
      When the agent calls the tool "app_set_property" with:
        """json
        {"target": "#secret", "property": "text", "value": "hunter2"}
        """
      Then the property "text" of "secret" is "hunter2"
      And the answer's "/value" is "<redacted>"
      And the answer does not contain "hunter2"

  Rule: A method is called and a signal is sent as the application would

    Scenario: A method is called with its arguments and answers its result
      When the agent calls the tool "app_invoke" with:
        """json
        {"target": "#root", "name": "add", "args": [2, 3]}
        """
      Then the answer's "/result" is 5

    Scenario: A method changes the application
      When the agent calls the tool "app_invoke" with:
        """json
        {"target": "#root", "name": "bump"}
        """
      Then the property "count" of "root" is 1
      And the property "twice" of "root" is 2

    Scenario: A signal is sent and its handlers run
      When the agent calls the tool "app_invoke" with:
        """json
        {"target": "#root", "name": "pinged", "args": ["the agent"]}
        """
      Then the property "chosen" of "root" is "the agent"

    Scenario: A method that throws is told as the application's fault
      When the agent calls the tool "app_invoke" with:
        """json
        {"target": "#root", "name": "fail"}
        """
      Then the agent is refused with "app_faulted"
      And the refusal mentions "no good"

    Scenario: A method nobody has is refused with its name
      When the agent calls the tool "app_invoke" with:
        """json
        {"target": "#root", "name": "teleport"}
        """
      Then the agent is refused with "not_found"
      And the refusal mentions "teleport"

    Scenario: A call says what changed
      When the agent calls the tool "app_invoke" with:
        """json
        {"target": "#root", "name": "add", "args": [1, 1]}
        """
      Then the answer's "/epoch" is at least 2
      And the answer's "/settled" is true

  Rule: A reload is asked for only where files are watched

    Scenario: An application that is not watched cannot be reloaded
      When the agent calls the tool "app_reload"
      Then the agent is refused with "unavailable"
      And the refusal mentions "--watch"
