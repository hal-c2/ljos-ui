# Sources:
#   docs/ARCHITECTURE.md sections 6.2, 19.4 and 19.7
#   https://www.erlang.org/doc/apps/observer/observer_ug.html (Trace Overview)
#   https://www.erlang.org/doc/apps/erts/erlang.html#trace/3
# The inspector is a Ljós extension: Qt has nothing to compare it with.

@ljos-only @headless
Feature: Tracing what happens to an object over time
  Erlang's observer traces a process: from the moment it is asked, each
  message the process gets is written down with when it came, and the
  list can be read while the process goes on. An object of a QML
  application is told things the same way, by its signals and by its
  properties changing, so an agent can trace an object and read, in the
  order they happened and with the application's time, the signals it
  emitted, the properties that changed with their new values, and its end.
  Nothing is written down for an object that is not traced, and the list
  is bounded: the oldest give way and are counted.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: top
        objectName: "root"
        width: 200; height: 100
        property int count: 0
        property real level: 0
        property bool open: true
        signal poked(int times)
        function poke() { count += 1; poked(count) }
        function many() { for (var i = 0; i < 1100; ++i) count += 1 }
        Item { objectName: "other"; property int n: 0 }
        Rectangle { objectName: "box"; width: 10; height: 10; border.color: top.open ? "red" : "blue" }
        TextInput { objectName: "secret"; width: 100; height: 20; echoMode: TextInput.Password }
        Loader {
          active: top.open
          sourceComponent: Item { objectName: "inner"; property int n: 0 }
        }
      }
      """
    And the inspector is connected

  Rule: A traced object's signals and changes are written down in order

    Scenario: Tracing an object says it is traced
      When the agent calls the tool "app_trace" with:
        """json
        {"target": "#root"}
        """
      Then the answer's "/tracing" has 1 entries
      And the answer's "/tracing/0/handle" matches "^@[0-9]+\.[0-9]+$"
      And the answer's "/tracing/0/name" is "Scenario \"root\""
      And the answer's "/events" has 0 entries

    Scenario: A change and then a signal are written down as they happened
      When the agent calls the tool "app_trace" with:
        """json
        {"target": "#root"}
        """
      And the method "poke" of "root" is called
      And the agent calls the tool "app_trace"
      Then the answer's "/events" has 2 entries
      And the answer's "/events/0/kind" is "change"
      And the answer's "/events/0/name" is "count"
      And the answer's "/events/0/value" is 1
      And the answer's "/events/1/kind" is "signal"
      And the answer's "/events/1/name" is "poked"
      And the answer's "/events/1/args/0" is 1
      And the answer's "/events/1/object" is "Scenario \"root\""

    Scenario: An event says when it happened by the application's clock
      When the agent calls the tool "app_trace" with:
        """json
        {"target": "#root"}
        """
      And the method "poke" of "root" is called
      And the agent calls the tool "app_trace"
      Then the answer's "/events/0/at_us" is at least 0
      And the answer's "/events/0/seq" is 1

    Scenario: Nothing is written down for an object that is not traced
      When the agent calls the tool "app_trace" with:
        """json
        {"target": "#root"}
        """
      And the property "n" of "other" is set to 3
      And the agent calls the tool "app_trace"
      Then the answer's "/events" has 0 entries

    Scenario: Nothing is written down before the trace started
      When the method "poke" of "root" is called
      And the agent calls the tool "app_trace" with:
        """json
        {"target": "#root"}
        """
      Then the answer's "/events" has 0 entries

    # An item let go of leaves its parent before it ends, as in Qt.
    Scenario: The end of a traced object is written down and it is traced no longer
      When the agent calls the tool "app_trace" with:
        """json
        {"target": "#inner"}
        """
      And the property "open" of "root" is set to false
      And the agent calls the tool "app_trace"
      Then the answer's "/events/0/name" is "parent"
      And the answer's "/events/0/value" is null
      And the answer's "/events/1/kind" is "destroyed"
      And the answer's "/tracing" has 0 entries

    # A number is written down with the digits a script prints it with,
    # which is how app_inspect shows a property's value.
    Scenario: A changed number keeps every digit it has
      When the agent calls the tool "app_trace" with:
        """json
        {"target": "#root"}
        """
      And the property "level" of "root" is set to 736.0003
      And the agent calls the tool "app_trace"
      Then the answer's "/events/0/name" is "level"
      And the answer's "/events/0/value" is 736.0003
      And the answer's text contains "= 736.0003"

    # A group's member has no change signal of its own; it changes all
    # the same, and its group is a part of the object already.
    Scenario: A member of a group is written down by its full name
      When the agent calls the tool "app_trace" with:
        """json
        {"target": "#box"}
        """
      And the property "open" of "root" is set to false
      And the agent calls the tool "app_trace"
      Then the answer's "/events/0/name" is "border.color"
      And the answer's "/events/0/value" is "#0000ff"
      And the answer's "/events" has 1 entries

  Rule: The list is read from where the last answer ended

    Scenario: What was read already is not given again
      When the agent calls the tool "app_trace" with:
        """json
        {"target": "#root"}
        """
      And the method "poke" of "root" is called
      And the agent calls the tool "app_trace"
      And the method "poke" of "root" is called
      And the agent calls the tool "app_trace" with:
        """json
        {"since": 2}
        """
      Then the answer's "/events" has 2 entries
      And the answer's "/events/0/seq" is 3
      And the answer's "/events/0/value" is 2
      And the answer's "/next_since" is 4

    Scenario: The oldest events give way and are counted
      When the agent calls the tool "app_trace" with:
        """json
        {"target": "#root"}
        """
      And the method "many" of "root" is called
      And the agent calls the tool "app_trace" with:
        """json
        {"max": 5}
        """
      Then the answer's "/dropped" is 100
      And the answer's "/events" has 5 entries
      And the answer's "/events/0/seq" is 101
      And the answer's "/more" is true

    Scenario: A watch is told that something was written down
      When the agent calls the tool "app_trace" with:
        """json
        {"target": "#root"}
        """
      And the method "poke" of "root" is called
      And the agent calls the tool "app_watch" with:
        """json
        {"since": {"traces": 0}, "topics": ["traces"], "wait_ms": 50}
        """
      Then the answer's "/waited" is false
      And the answer's "/changed/0" is "traces"
      And the answer's "/counts/traces" is 2

  Rule: A trace is stopped by the agent

    Scenario: A stopped trace writes nothing more and keeps what it had
      When the agent calls the tool "app_trace" with:
        """json
        {"target": "#root"}
        """
      And the method "poke" of "root" is called
      And the agent calls the tool "app_trace" with:
        """json
        {"stop": true}
        """
      And the method "poke" of "root" is called
      And the agent calls the tool "app_trace"
      Then the answer's "/tracing" has 0 entries
      And the answer's "/events" has 2 entries

    Scenario: Stopping one object keeps the others traced
      When the agent calls the tool "app_trace" with:
        """json
        {"target": "#root"}
        """
      And the agent calls the tool "app_trace" with:
        """json
        {"target": "#other"}
        """
      And the agent calls the tool "app_trace" with:
        """json
        {"target": "#root", "stop": true}
        """
      Then the answer's "/tracing" has 1 entries
      And the answer's "/tracing/0/name" is "Item \"other\""

    Scenario: A target that names nothing is refused
      When the agent calls the tool "app_trace" with:
        """json
        {"target": "#nobody"}
        """
      Then the agent is refused with "not_found"

    Scenario: More than the most events an answer holds is refused
      When the agent calls the tool "app_trace" with:
        """json
        {"max": 501}
        """
      Then the agent is refused with "invalid_argument"

  Rule: A trace a client holds ends when that client is gone

    # Each call is answered on its own, so a client is known to be there
    # only while it waits for the application with `app_watch`. A client
    # that follows the application asks again as soon as it is answered;
    # one that has not asked again within the time its last watch said it
    # would wait, and at least a second, is gone, and so are its traces.
    Scenario: A held trace lasts while its holder watches the application
      When the agent calls the tool "app_trace" with:
        """json
        {"target": "#root", "holder": "observer"}
        """
      And the agent calls the tool "app_watch" with:
        """json
        {"since": {"traces": 0}, "topics": ["traces"], "wait_ms": 50, "holder": "observer"}
        """
      And the agent calls the tool "app_trace"
      Then the answer's "/tracing" has 1 entries

    Scenario: A held trace ends once its holder stops watching
      When the agent calls the tool "app_trace" with:
        """json
        {"target": "#root", "holder": "observer"}
        """
      And the agent calls the tool "app_trace" with:
        """json
        {"target": "#other"}
        """
      And the agent calls the tool "app_watch" with:
        """json
        {"since": {"traces": 0}, "topics": ["traces"], "wait_ms": 50, "holder": "observer"}
        """
      And the agent calls the tool "app_watch" with:
        """json
        {"since": {"traces": 0}, "topics": ["traces"], "wait_ms": 1500}
        """
      And the agent calls the tool "app_trace"
      Then the answer's "/tracing" has 1 entries
      And the answer's "/tracing/0/name" is "Item \"other\""

    Scenario: The traces of one holder are stopped together
      When the agent calls the tool "app_trace" with:
        """json
        {"target": "#root", "holder": "observer"}
        """
      And the agent calls the tool "app_trace" with:
        """json
        {"target": "#secret", "holder": "observer"}
        """
      And the agent calls the tool "app_trace" with:
        """json
        {"target": "#other"}
        """
      And the agent calls the tool "app_trace" with:
        """json
        {"stop": true, "holder": "observer"}
        """
      Then the answer's "/tracing" has 1 entries
      And the answer's "/tracing/0/name" is "Item \"other\""

  Rule: What a field hides is not told by its trace

    # Its length is told, as the field shows as many dots.
    Scenario: The text of a field that hides it is masked in its changes
      When the agent calls the tool "app_trace" with:
        """json
        {"target": "#secret"}
        """
      And the property "text" of "secret" is set to "hunter2"
      And the agent calls the tool "app_trace"
      Then the answer's "/events/1/name" is "text"
      And the answer's "/events/1/value" is "<redacted>"
      And the answer does not contain "hunter2"
