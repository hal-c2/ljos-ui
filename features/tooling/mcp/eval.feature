# Sources:
#   docs/ARCHITECTURE.md sections 3.4, 19.4, 19.6 and 19.7
#   docs/research/10-mcp.md
#   https://doc.qt.io/qt-6/qtqml-javascript-expressions.html
#   https://doc.qt.io/qt-6/qtqml-javascript-hostenvironment.html
# The inspector is a Ljós extension: Qt has nothing to compare it with.

@ljos-only @headless
Feature: An agent that may run script
  With the capability `eval`, and only with it, an agent can run JavaScript
  in the context of an object: the object's properties are named as a
  binding written in it names them. The script has a time to run in, and
  what it answers is written by the same bounded serialiser as
  `app_inspect`, in at most 64 KB. A script can do what the application can,
  so `eval` is a capability of its own that nothing else grants.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: top
        objectName: "root"
        width: 200; height: 100
        property int count: 3
        property string label: "hello"
        property string mirror: pw.text
        Rectangle {
          objectName: "save"
          width: 80; height: 30
          Accessible.role: Accessible.Button
          Accessible.name: "Save"
        }
        TextInput {
          id: pw
          objectName: "pw"
          y: 40; width: 100; height: 20
          echoMode: TextInput.Password
          text: "hunter2"
          Accessible.name: "Password"
        }
      }
      """
    And the inspector is connected with the capabilities "inspect, eval"
    And the agent has requested a snapshot

  Rule: The script runs in the object it is given

    Scenario: The properties of the object are named as a binding names them
      When the agent calls the tool "app_eval" with:
        """json
        {"target": "#root", "source": "count * 2 + width"}
        """
      Then the answer's "/value" is 206

    Scenario: A reference names the object as a locator does
      When the agent calls the tool "app_eval" on the reference of the "button" named "Save" with:
        """json
        {"source": "width + height"}
        """
      Then the answer's "/value" is 110

    Scenario: Without an object the script runs in the global scope
      When the agent calls the tool "app_eval" with:
        """json
        {"source": "[1, 2, 3].map(x => x * x)"}
        """
      Then the answer's "/value" is [1, 4, 9]

    Scenario: An object is answered as the inspector writes one
      When the agent calls the tool "app_eval" with:
        """json
        {"target": "#root", "source": "({n: count, tags: ['a', 'b'], none: null})"}
        """
      Then the answer's "/value/n" is 3
      And the answer's "/value/tags/1" is "b"
      And the answer's "/value/none" is null

    Scenario: A script can change the application, which is why it is a capability
      When the agent calls the tool "app_eval" with:
        """json
        {"target": "#root", "source": "count = 11"}
        """
      Then the property "count" of "root" is 11

    Scenario: A script that throws is told with what it threw
      When the agent calls the tool "app_eval" with:
        """json
        {"target": "#root", "source": "nothing.here"}
        """
      Then the agent is refused with "invalid_argument"
      And the refusal mentions "ReferenceError"

    Scenario: An item that is gone is a stale reference
      When the agent calls the tool "app_eval" with:
        """json
        {"target": "e9999", "source": "1"}
        """
      Then the agent is told the reference is stale

  Rule: What the script answers is bounded

    Scenario: A long list is cut and says so
      When the agent calls the tool "app_eval" with:
        """json
        {"source": "Array.from({length: 1000}, (_, i) => i)"}
        """
      Then the answer's "/truncated" is true
      And the answer's "/value/0" is 0
      And the answer's "/value/99" is 99

    Scenario: A long string is cut and says so
      When the agent calls the tool "app_eval" with:
        """json
        {"source": "'x'.repeat(100000)"}
        """
      Then the answer's "/truncated" is true
      And the answer is at most 65536 bytes

    Scenario: What is deeper than the serialiser goes is told by its kind
      When the agent calls the tool "app_eval" with:
        """json
        {"source": "({a: {b: {c: {d: {e: {f: {g: {h: 1}}}}}}}})"}
        """
      Then the answer's "/truncated" is true

    Scenario: A function is told by a word and not followed
      When the agent calls the tool "app_eval" with:
        """json
        {"source": "({f: function () {}})"}
        """
      Then the answer's "/value/f" is "[function]"

  Rule: The script has a time to run in

    Scenario: A script that never ends is stopped and the application goes on
      When the agent calls the tool "app_eval" with:
        """json
        {"source": "while (true) {}", "timeout_ms": 200}
        """
      Then the agent is refused with "timeout"
      And the refusal mentions "200 ms"
      When the agent calls the tool "app_status"
      Then the answer's "/windows/0/width" is 400

    Scenario: A time that is too long is refused
      When the agent calls the tool "app_eval" with:
        """json
        {"source": "1", "timeout_ms": 3600000}
        """
      Then the agent is refused with "invalid_argument"
      And the refusal mentions "timeout_ms"

  Rule: Script cannot read around the mask

    Scenario: The text of a field that hides it is not given to a script that is run in it
      When the agent calls the tool "app_eval" with:
        """json
        {"target": "#pw", "source": "text"}
        """
      Then the agent is refused with "capability_denied"
      And the refusal mentions "hides"
      And the answer does not contain "hunter2"

    Scenario: The text of a field that hides it is masked wherever a script finds it
      When the agent calls the tool "app_eval" with:
        """json
        {"target": "#root", "source": "mirror"}
        """
      Then the answer's "/value" is "<redacted>"
      And the answer does not contain "hunter2"

    Scenario: The mask holds inside what the script builds
      When the agent calls the tool "app_eval" with:
        """json
        {"target": "#root", "source": "({inside: ['<' + mirror + '>'], kept: label})"}
        """
      Then the answer's "/value/inside/0" is "<<redacted>>"
      And the answer's "/value/kept" is "hello"
      And the answer does not contain "hunter2"
