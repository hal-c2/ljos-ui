# Sources:
#   docs/ARCHITECTURE.md sections 6.2 and 19.4 (`app_objects`' `models`
#   and `model`, `app_set_property`'s `row`)
#   https://www.erlang.org/doc/apps/observer/observer_ug.html (Table Viewer)
# The inspector is a Ljós extension: Qt has nothing to compare it with.

@ljos-only @headless
Feature: An agent reads and edits the application's models
  Erlang's observer has a table viewer: it lists a node's tables, shows
  the rows of one a page at a time, edits a row, and shows the table
  again as it changes. The tables of a QML application are its models:
  a ListModel, the number or array a Repeater or a view is given, an
  ObjectModel, a DelegateModel, and a list model written in Rust. Each is
  listed with the object that has it, its rows and its roles, and the
  views that show it; its rows are read a page at a time; a cell is
  written as a delegate writes `model.role`; and a client that follows a
  model is told when it changed. Nothing is subscribed to a model unless
  a client follows it, and only while that client watches.

  Background:
    Given a list model "pages" of 30 rows that reads 30 at a time, 0 ms after a view asks for more
    And the QML:
      """qml
      import QtQuick
      import QtQml.Models
      Item {
        objectName: "root"
        width: 200; height: 400
        function grow() { fruit.append({ name: "Date", cost: 3 }) }
        function fill() { for (var i = 0; i < 600; ++i) fruit.append({ name: "n" + i, cost: i }) }
        ListModel {
          id: fruit; objectName: "fruit"
          ListElement { name: "Apple"; cost: 2 } ListElement { name: "Banana"; cost: 1 }
          ListElement { name: "Cherry"; cost: 5 }
        }
        Column {
          Repeater { objectName: "counter"; model: 3; Text { text: index } }
          Repeater { objectName: "words"; model: ["one", "two"]; Text { text: modelData } }
          ListView { objectName: "list"; width: 100; height: 60; model: fruit; delegate: Text { text: name } }
          ListView { objectName: "parts"; width: 100; height: 40
            model: ObjectModel { objectName: "bits"; Text { text: "a" } Text { text: "b" } } }
          ListView { objectName: "grouped"; width: 100; height: 40
            model: DelegateModel { objectName: "dm"; model: fruit; delegate: Text { text: name } } }
          ListView { objectName: "paged"; width: 100; height: 40; model: pages; delegate: Text { text: display } }
        }
      }
      """
    And the inspector is connected with the capabilities "inspect,mutate"

  Rule: Every model is listed with what has it, its rows and roles, and its views

    Scenario: A ListModel is listed by its object, with the views that show it
      When the agent calls the tool "app_objects" with:
        """json
        {"models": true}
        """
      Then the answer's "/models" has 6 entries
      And the answer's text contains "ListModel \"fruit\" 3 rows, roles cost, name, writable, shown by ListView \"list\""

    Scenario: A number and an array are the models of the view given them
      When the agent calls the tool "app_objects" with:
        """json
        {"models": true}
        """
      Then the answer's text contains "number 3 rows, of Repeater \"counter\""
      And the answer's text contains "array 2 rows, of Repeater \"words\""

    Scenario: An ObjectModel, a DelegateModel and a model written in Rust are listed
      When the agent calls the tool "app_objects" with:
        """json
        {"models": true}
        """
      Then the answer's text contains "ObjectModel \"bits\" 2 rows"
      And the answer's text contains "DelegateModel \"dm\" 3 rows"
      And the answer's text contains "QtObject 30 rows"

  Rule: A model's rows are read a page at a time

    Scenario: The rows of a model are read with their roles
      When the agent calls the tool "app_objects" with:
        """json
        {"model": "#fruit"}
        """
      Then the answer's "/type" is "ListModel"
      And the answer's "/count" is 3
      And the answer's "/roles" is ["cost", "name"]
      And the answer's "/writable" is true
      And the answer's "/rows" has 3 entries
      And the answer's "/rows/1/index" is 1
      And the answer's "/rows/1/values/name" is "Banana"
      And the answer's "/rows/1/values/cost" is 1

    Scenario: A view stands for the model it shows
      When the agent calls the tool "app_objects" with:
        """json
        {"model": "#counter"}
        """
      Then the answer's "/type" is "number"
      And the answer's "/writable" is false
      And the answer's "/count" is 3
      And the answer's "/rows/2/values/modelData" is 2

    Scenario: A page is read from an offset
      When the method "fill" of "root" is called
      And the agent calls the tool "app_objects" with:
        """json
        {"model": "#fruit", "offset": 100, "limit": 5}
        """
      Then the answer's "/count" is 603
      And the answer's "/rows" has 5 entries
      And the answer's "/rows/0/index" is 100
      And the answer's "/next_offset" is 105

    Scenario: A page holds 50 rows unless told otherwise
      When the method "fill" of "root" is called
      And the agent calls the tool "app_objects" with:
        """json
        {"model": "#fruit"}
        """
      Then the answer's "/rows" has 50 entries
      And the answer's "/next_offset" is 50

    Scenario: More than the most rows a page holds is refused
      When the agent calls the tool "app_objects" with:
        """json
        {"model": "#fruit", "limit": 501}
        """
      Then the agent is refused with "invalid_argument"
      And the refusal mentions "limit"

    Scenario: An object that is no model and shows none is refused
      When the agent calls the tool "app_objects" with:
        """json
        {"model": "#root"}
        """
      Then the agent is refused with "invalid_argument"
      And the refusal mentions "no model"

  Rule: A cell is written as a delegate writes `model.role`

    Scenario: A cell of a ListModel is written
      When the agent calls the tool "app_set_property" with:
        """json
        {"target": "#fruit", "row": 1, "property": "name", "value": "Blueberry"}
        """
      Then the answer's "/value" is "Blueberry"
      And the answer's "/previous" is "Banana"
      And the property "count" of "list" is 3

    Scenario: A written cell is what the view's model reads after
      When the agent calls the tool "app_set_property" with:
        """json
        {"target": "#fruit", "row": 1, "property": "name", "value": "Blueberry"}
        """
      And the agent calls the tool "app_objects" with:
        """json
        {"model": "#list", "offset": 1, "limit": 1}
        """
      Then the answer's "/rows/0/values/name" is "Blueberry"

    Scenario: A role the model does not have is refused
      When the agent calls the tool "app_set_property" with:
        """json
        {"target": "#fruit", "row": 1, "property": "colour", "value": "blue"}
        """
      Then the agent is refused with "not_actionable"
      And the refusal mentions "colour"

    Scenario: A row past the last is refused
      When the agent calls the tool "app_set_property" with:
        """json
        {"target": "#fruit", "row": 3, "property": "name", "value": "Fig"}
        """
      Then the agent is refused with "invalid_argument"
      And the refusal mentions "3 rows"

    Scenario: A model that takes no writes refuses them
      When the agent calls the tool "app_set_property" with:
        """json
        {"target": "#counter", "row": 0, "property": "modelData", "value": 7}
        """
      Then the agent is refused with "not_actionable"

    Scenario: A cell is not bound or reverted
      When the agent calls the tool "app_set_property" with:
        """json
        {"target": "#fruit", "row": 0, "property": "name", "binding": "'x'"}
        """
      Then the agent is refused with "invalid_argument"
      And the refusal mentions "row"

  Rule: A client that follows a model is told when it changed

    # A model is followed by reading it with a `holder`: from then on
    # each change of that model moves the watch's `models` count, until
    # the holder follows another model or stops watching.
    Scenario: A followed model's change is told to a watch
      When the agent calls the tool "app_objects" with:
        """json
        {"model": "#fruit", "holder": "observer"}
        """
      And the method "grow" of "root" is called
      And the agent calls the tool "app_watch" with:
        """json
        {"since": {"models": 0}, "topics": ["models"], "wait_ms": 50, "holder": "observer"}
        """
      Then the answer's "/waited" is false
      And the answer's "/changed/0" is "models"
      And the answer's "/counts/models" is 1

    Scenario: A written cell is told to a watch too
      When the agent calls the tool "app_objects" with:
        """json
        {"model": "#fruit", "holder": "observer"}
        """
      And the agent calls the tool "app_set_property" with:
        """json
        {"target": "#fruit", "row": 0, "property": "cost", "value": 9}
        """
      And the agent calls the tool "app_watch" with:
        """json
        {"since": {"models": 0}, "topics": ["models"], "wait_ms": 50, "holder": "observer"}
        """
      Then the answer's "/counts/models" is 1

    Scenario: A model nobody follows tells no watch
      When the agent calls the tool "app_objects" with:
        """json
        {"model": "#fruit"}
        """
      And the method "grow" of "root" is called
      And the agent calls the tool "app_watch" with:
        """json
        {"since": {"models": 0}, "topics": ["models"], "wait_ms": 50}
        """
      Then the answer's "/waited" is true
      And the answer's "/counts/models" is 0

    Scenario: A model is followed no longer once its holder stops watching
      When the agent calls the tool "app_objects" with:
        """json
        {"model": "#fruit", "holder": "observer"}
        """
      And the agent calls the tool "app_watch" with:
        """json
        {"since": {"models": 0}, "topics": ["models"], "wait_ms": 50, "holder": "observer"}
        """
      And the agent calls the tool "app_watch" with:
        """json
        {"since": {"models": 0}, "topics": ["models"], "wait_ms": 1500}
        """
      And the method "grow" of "root" is called
      And the agent calls the tool "app_watch" with:
        """json
        {"since": {"models": 0}, "topics": ["models"], "wait_ms": 50}
        """
      Then the answer's "/counts/models" is 0

    Scenario: A holder follows one model at a time
      When the agent calls the tool "app_objects" with:
        """json
        {"model": "#fruit", "holder": "observer"}
        """
      And the agent calls the tool "app_objects" with:
        """json
        {"model": "#words", "holder": "observer"}
        """
      And the method "grow" of "root" is called
      And the agent calls the tool "app_watch" with:
        """json
        {"since": {"models": 0}, "topics": ["models"], "wait_ms": 50, "holder": "observer"}
        """
      Then the answer's "/counts/models" is 0
