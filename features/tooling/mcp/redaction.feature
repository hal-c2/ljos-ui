# Sources:
#   docs/ARCHITECTURE.md sections 19.4 and 19.7
#   docs/research/10-mcp.md
#   https://doc.qt.io/qt-6/qml-qtquick-textinput.html#echoMode-prop
# The inspector is a Ljós extension: Qt has nothing to compare it with.

@ljos-only @headless
Feature: What an agent that acts is not told
  A field that hides its text is not echoed by what the agent does to it,
  and script is not a way around the mask. When the inspector is started
  with `--mcp-redact` every editable value is masked, and script is refused
  altogether: what a script can compute from a value cannot be told from
  the value, so no script is run while the mask is asked for. A method or
  a signal handler of the application is script too, and is not run then.
  What an act answers, a refusal included, never holds a masked value.
  Nor does what the application logged or threw: a line or a fault is
  masked with every secret the inspector has seen a field hold, so a secret
  that was changed or cleared since is not told either.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"; width: 300; height: 100
        property string note: "plain"
        function reveal() { note = "ran"; return memo.text }
        function scramble() { note = "ran"; return memo.text.split("").reverse().join("") }
        function blurt() { note = "ran"; throw new Error("the note is " + memo.text) }
        function blurtBackwards() { note = "ran"; throw new Error(memo.text.split("").reverse().join("")) }
        function blurtPin() { throw new Error("the pin is " + pin.text) }
        function logPin() { console.log("the pin is " + pin.text); console.log("an ordinary line"); console.log("the memo is " + memo.text) }
        signal asked()
        onAsked: { note = "ran"; throw new Error(memo.text) }
        TextInput {
          objectName: "pw"; width: 120; height: 20
          echoMode: TextInput.Password; Accessible.name: "Password"
        }
        TextInput {
          id: memo; objectName: "memo"; text: "private note"
          y: 40; width: 120; height: 20; Accessible.name: "Memo"
        }
        TextInput {
          id: pin; objectName: "pin"; text: "s3cret-pin"
          y: 70; width: 120; height: 20; echoMode: TextInput.Password
        }
      }
      """

  Rule: Typing is not echoed for a field that hides it

    Scenario: What is typed into a password field is not told back
      Given the inspector is connected with the capabilities "inspect, input"
      When the agent calls the tool "app_type" with:
        """json
        {"text": "hunter2", "target": "#pw"}
        """
      Then the property "text" of "pw" is "hunter2"
      And the answer does not contain "hunter2"

    Scenario: What changed is not told with the text either
      Given the inspector is connected with the capabilities "inspect, input"
      And the agent has requested a snapshot
      When the agent calls the tool "app_type" with:
        """json
        {"text": "hunter2", "target": "#pw"}
        """
      Then the answer does not contain "hunter2"

    Scenario: What is typed into a field that shows its text is told
      Given the inspector is connected with the capabilities "inspect, input"
      When the agent calls the tool "app_type" with:
        """json
        {"text": "x", "target": "#memo"}
        """
      Then the answer's "/typed" is "x"

  Rule: The mask holds in the calls that change a value

    Scenario: The value set into a field that hides it is not told back
      Given the inspector is connected with the capabilities "inspect, mutate"
      When the agent calls the tool "app_set_property" with:
        """json
        {"target": "#pw", "property": "text", "value": "hunter2"}
        """
      Then the answer does not contain "hunter2"

  Rule: Script is refused while the mask is asked for

    Scenario: A script is refused and the refusal says why
      Given the inspector is connected with the capabilities "inspect, eval" and masks every editable value
      When the agent calls the tool "app_eval" with:
        """json
        {"source": "1 + 1"}
        """
      Then the agent is refused with "capability_denied"
      And the refusal mentions "--mcp-redact"

    Scenario: The refusal does not depend on what the script reads
      Given the inspector is connected with the capabilities "inspect, eval" and masks every editable value
      When the agent calls the tool "app_eval" with:
        """json
        {"target": "#memo", "source": "text"}
        """
      Then the agent is refused with "capability_denied"
      And the answer does not contain "private note"

    Scenario: A property set in a field is masked when the mask is asked for
      Given the inspector is connected with the capabilities "inspect, mutate" and masks every editable value
      When the agent calls the tool "app_set_property" with:
        """json
        {"target": "#memo", "property": "text", "value": "fresh note"}
        """
      Then the answer does not contain "fresh note"
      And the answer's "/value" is "<redacted>"

  Rule: A method or a signal is not run while the mask is asked for

    Scenario Outline: <what> is refused before it runs and the refusal tells nothing of it
      Given the inspector is connected with the capabilities "inspect, mutate" and masks every editable value
      When the agent calls the tool "app_invoke" with:
        """json
        {"target": "#root", "name": "<name>"}
        """
      Then the agent is refused with "capability_denied"
      And the refusal mentions "--mcp-redact"
      And the property "note" of "root" is "plain"
      And the answer does not contain "private note"
      And the answer does not contain "eton etavirp"

      Examples:
        | what                                       | name           |
        | A method that answers a value              | reveal         |
        | A method that answers a value it changed   | scramble       |
        | A method that throws a value               | blurt          |
        | A method that throws a value it changed    | blurtBackwards |
        | A signal whose handler throws a value      | asked          |

    Scenario: A step of a batch that calls a method is refused the same way
      Given the inspector is connected with the capabilities "inspect, input, mutate" and masks every editable value
      When the agent calls the tool "app_batch" with:
        """json
        {"steps": [{"tool": "app_invoke", "args": {"target": "#root", "name": "blurt"}}]}
        """
      Then the answer's "/failed/code" is "capability_denied"
      And the property "note" of "root" is "plain"
      And the answer does not contain "private note"

  Rule: A fault is told without what a hidden field holds

    Scenario: A method that throws what a password field holds is told without it
      Given the inspector is connected with the capabilities "inspect, mutate"
      When the agent calls the tool "app_invoke" with:
        """json
        {"target": "#root", "name": "blurtPin"}
        """
      Then the agent is refused with "app_faulted"
      And the refusal mentions "the pin is <redacted>"
      And the answer does not contain "s3cret-pin"

    Scenario: A step of a batch that throws it is told without it either
      Given the inspector is connected with the capabilities "inspect, input, mutate"
      When the agent calls the tool "app_batch" with:
        """json
        {"steps": [{"tool": "app_invoke", "args": {"target": "#root", "name": "blurtPin"}}]}
        """
      Then the answer's "/failed/code" is "app_faulted"
      And the answer does not contain "s3cret-pin"

  Rule: Batch holds the same masks

    Scenario: A step of a batch is masked as it would be alone
      Given the inspector is connected with the capabilities "inspect, input"
      When the agent calls the tool "app_batch" with:
        """json
        {"steps": [{"tool": "app_type", "args": {"text": "hunter2", "target": "#pw"}}]}
        """
      Then the answer does not contain "hunter2"

  Rule: What the application logs or throws is told without what a hidden field holds

    Scenario: A line logged before the agent first looked is told without the secret
      Given the inspector is connected with the capabilities "inspect"
      When the method "logPin" of "root" is called
      And the agent calls the tool "app_logs"
      Then the answer does not contain "s3cret-pin"
      And the answer's "/entries/0/message" is "the pin is <redacted>"
      And the answer's "/entries/1/message" is "an ordinary line"
      And the answer's text contains "the pin is <redacted>"
      And the answer's text contains "an ordinary line"

    Scenario: The log resource is told without the secret too
      Given the inspector is connected with the capabilities "inspect"
      When the method "logPin" of "root" is called
      And the agent reads the resource "ljos://app/logs"
      Then the answer does not contain "s3cret-pin"
      And the answer's text contains "the pin is <redacted>"
      And the answer's text contains "an ordinary line"

    Scenario: A line logged after the agent looked is told without the secret
      Given the inspector is connected with the capabilities "inspect"
      And the agent has requested a snapshot
      When the method "logPin" of "root" is called
      And the agent calls the tool "app_logs"
      Then the answer does not contain "s3cret-pin"
      And the answer's "/entries/0/message" is "the pin is <redacted>"
      And the answer's "/entries/1/message" is "an ordinary line"

    Scenario Outline: A line logged before the secret <change> is still told without it
      Given the inspector is connected with the capabilities "inspect"
      When the method "logPin" of "root" is called
      And the agent calls the tool "app_status"
      And the property "text" of "pin" is set to <value>
      And the agent calls the tool "app_logs"
      Then the answer does not contain "s3cret-pin"
      And the answer's "/entries/1/message" is "an ordinary line"

      Examples:
        | change     | value     |
        | changes    | "new-pin" |
        | is cleared | ""        |

    Scenario: A fault thrown before the agent first looked is told without the secret
      Given the inspector is connected with the capabilities "inspect"
      When the method "blurtPin" of "root" is called
      And the agent calls the tool "app_processes" with:
        """json
        {"kind": "fault"}
        """
      Then the answer does not contain "s3cret-pin"
      And the answer's "/faults/0/message" matches "the pin is <redacted>"
      And the answer's text contains "the pin is <redacted>"

    Scenario Outline: A fault thrown before the secret <change> is still told without it
      Given the inspector is connected with the capabilities "inspect"
      And the agent has requested a snapshot
      When the method "blurtPin" of "root" is called
      And the property "text" of "pin" is set to <value>
      And the agent calls the tool "app_processes" with:
        """json
        {"kind": "fault"}
        """
      Then the answer does not contain "s3cret-pin"
      And the answer's "/faults/0/fault" is "JsException"

      Examples:
        | change     | value     |
        | changes    | "new-pin" |
        | is cleared | ""        |

    Scenario: A line that repeats an editable value is masked when the mask is asked for
      Given the inspector is connected with the capabilities "inspect" and masks every editable value
      When the method "logPin" of "root" is called
      And the agent calls the tool "app_logs"
      Then the answer does not contain "private note"
      And the answer does not contain "s3cret-pin"
      And the answer's "/entries/2/message" is "the memo is <redacted>"
      And the answer's text contains "an ordinary line"
