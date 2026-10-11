# Sources:
#   docs/ARCHITECTURE.md sections 3.4, 19.4 and 19.6
#   docs/research/10-mcp.md
#   https://doc.qt.io/qt-6/qtqml-syntax-propertybinding.html
# The inspector is a Ljós extension: Qt has nothing to compare it with.

@ljos-only @headless
Feature: What a property depends on
  An agent that wants to know why a value is what it is can ask for the
  dependencies of one property: the properties its binding read when it last
  ran, the bindings that read it, and the tier the binding runs at. This
  only reads, so it needs no more than the capability `inspect`.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: top
        objectName: "root"
        width: 200; height: 100
        property int count: 2
        property int twice: count * 2
        property string label: "n=" + twice
        property int plain: 5
        Rectangle {
          objectName: "box"
          width: top.twice
          height: 10
        }
      }
      """
    And the inspector is connected
    And the agent has requested a snapshot

  Rule: A binding says what it read

    Scenario: A property is told with the tier of its binding
      When the agent calls the tool "app_inspect" with:
        """json
        {"target": "#root", "dependencies": "twice"}
        """
      Then the answer's "/dependencies/property" is "twice"
      And the answer's "/dependencies/bound" is true
      And the answer's "/dependencies/tier" is "1"

    Scenario: What a binding read is listed with the object and the property
      When the agent calls the tool "app_inspect" with:
        """json
        {"target": "#root", "dependencies": "twice"}
        """
      Then the answer's "/dependencies/reads/0/property" is "count"
      And the answer's "/dependencies/reads/0/object_name" is "root"

    Scenario: What reads a property is listed, across objects
      When the agent calls the tool "app_inspect" with:
        """json
        {"target": "#root", "dependencies": "twice"}
        """
      Then the answer's "/dependencies/read_by" has 2 entries

    Scenario: A property that nothing binds reads nothing
      When the agent calls the tool "app_inspect" with:
        """json
        {"target": "#root", "dependencies": "plain"}
        """
      Then the answer's "/dependencies/bound" is false
      And the answer's "/dependencies/reads" has 0 entries

    Scenario: The graph follows an assignment that replaced the binding
      Given the inspector is connected with the capabilities "inspect, mutate"
      And the agent has requested a snapshot
      When the agent calls the tool "app_set_property" with:
        """json
        {"target": "#root", "property": "twice", "value": 99}
        """
      And the agent calls the tool "app_inspect" with:
        """json
        {"target": "#root", "dependencies": "twice"}
        """
      Then the answer's "/dependencies/bound" is false
      And the answer's "/dependencies/reads" has 0 entries

  Rule: A property that is not there is told with its name

    Scenario: A misspelt property is refused with its name
      When the agent calls the tool "app_inspect" with:
        """json
        {"target": "#root", "dependencies": "cuont"}
        """
      Then the agent is refused with "not_found"
      And the refusal mentions "cuont"

    Scenario: A reference to an item that is gone is stale
      When the agent calls the tool "app_inspect" with:
        """json
        {"target": "e9999", "dependencies": "twice"}
        """
      Then the agent is told the reference is stale
