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
  the value, so no script is run while the mask is asked for.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        width: 300; height: 100
        property string note: "plain"
        TextInput {
          objectName: "pw"
          width: 120; height: 20
          echoMode: TextInput.Password
          Accessible.name: "Password"
        }
        TextInput {
          objectName: "memo"
          y: 40; width: 120; height: 20
          text: "private note"
          Accessible.name: "Memo"
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

  Rule: Batch holds the same masks

    Scenario: A step of a batch is masked as it would be alone
      Given the inspector is connected with the capabilities "inspect, input"
      When the agent calls the tool "app_batch" with:
        """json
        {"steps": [{"tool": "app_type", "args": {"text": "hunter2", "target": "#pw"}}]}
        """
      Then the answer does not contain "hunter2"
