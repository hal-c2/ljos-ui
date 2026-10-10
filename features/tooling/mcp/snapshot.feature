# Sources:
#   docs/ARCHITECTURE.md sections 19.4 and 19.9
#   docs/research/10-mcp.md
# These scenarios use steps that are not built yet; they are listed as planned
# in features/README.md.

@ljos-only @headless @backlog
Feature: Inspector snapshot
  An agent reads the running application as a compact accessibility snapshot.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        width: 200; height: 100
        Rectangle {
          objectName: "save"
          width: 80; height: 30
          Accessible.role: Accessible.Button
          Accessible.name: "Save"
        }
      }
      """
    And the inspector is connected

  Scenario: The snapshot lists named controls with a reference each
    When the agent requests a snapshot
    Then the snapshot has a "button" named "Save" with a reference

  Scenario: A reference to a destroyed item is reported as stale
    Given the agent has requested a snapshot
    When the object "save" is destroyed
    And the agent clicks the reference of the "button" named "Save"
    Then the agent is told the reference is stale

  Scenario: Input goes through the real input path
    Given the agent has requested a snapshot
    When the agent clicks the reference of the "button" named "Save"
    Then the signal "clicked" of "save" has fired once

  Scenario: A password field never reveals its text
    Given the file "Login.qml":
      """qml
      import QtQuick
      TextInput { objectName: "pw"; echoMode: TextInput.Password; text: "hunter2" }
      """
    When "Login.qml" is loaded
    And the agent requests a snapshot
    Then the snapshot does not contain "hunter2"
