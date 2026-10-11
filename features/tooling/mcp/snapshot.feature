# Sources:
#   docs/ARCHITECTURE.md sections 19.4, 19.6, 19.7 and 19.9
#   docs/research/10-mcp.md
#   https://doc.qt.io/qt-6/qml-qtquick-accessible.html
#   https://doc.qt.io/qt-6/qml-qtquick-textinput.html#echoMode-prop
# The inspector is a Ljós extension: Qt has nothing to compare it with. The
# two scenarios that click wait for the input capability, which is not part
# of the read-only inspector.

@ljos-only @headless
Feature: Inspector snapshot
  An agent reads the running application as a compact outline of what an
  assistive technology would be told: one line per item, with its role, its
  name and a reference the agent can use in its next request.

  Rule: The outline names every exposed item once

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

    Scenario: The window is the first line and the items are indented below it
      When the agent requests a snapshot
      Then the snapshot is:
        """text
        - window [e1] 400x400
          - button "Save" [e2] obj=save
        """

    Scenario: An item keeps its reference from one snapshot to the next
      Given the agent has requested a snapshot
      When the property "width" of "save" is set to 90
      And the agent requests a snapshot
      Then the snapshot contains "- button \"Save\" [e2] obj=save"

    @backlog
    Scenario: A reference to a destroyed item is reported as stale
      Given the agent has requested a snapshot
      When the object "save" is destroyed
      And the agent clicks the reference of the "button" named "Save"
      Then the agent is told the reference is stale

    @backlog
    Scenario: Input goes through the real input path
      Given the agent has requested a snapshot
      When the agent clicks the reference of the "button" named "Save"
      Then the signal "clicked" of "save" has fired once

  Rule: A line says what the item is, how it is called and what state it is in

    Scenario: The id, the object name, the value and the state follow the name
      Given the QML:
        """qml
        import QtQuick
        Item {
          width: 300; height: 200
          Rectangle {
            id: newButton
            width: 80; height: 30
            Accessible.role: Accessible.Button
            Accessible.name: "New note"
          }
          Rectangle {
            objectName: "delete"
            y: 40; width: 80; height: 30
            enabled: false
            Accessible.role: Accessible.Button
            Accessible.name: "Delete"
          }
          TextInput {
            y: 80; width: 120; height: 20
            text: "milk"; focus: true
            Accessible.name: "Search"
          }
        }
        """
      And the inspector is connected
      When the agent requests a snapshot
      Then the snapshot is:
        """text
        - window [e1] 400x400
          - button "New note" [e2] id=newButton
          - button "Delete" [e3] obj=delete disabled
          - textinput "Search" value="milk" [e4] focused
        """

    Scenario: A control that can be used but has no name is flagged
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          objectName: "anonymous"
          width: 80; height: 30
          Accessible.role: Accessible.Button
        }
        """
      And the inspector is connected
      When the agent requests a snapshot
      Then the snapshot contains "- button [e2] obj=anonymous ⚠unnamed"

    Scenario: Items that are not shown are counted and not listed
      Given the QML:
        """qml
        import QtQuick
        Item {
          width: 200; height: 100
          Item {
            visible: false
            Rectangle { Accessible.role: Accessible.Button; Accessible.name: "One" }
            Rectangle { Accessible.role: Accessible.Button; Accessible.name: "Two" }
          }
          Rectangle { Accessible.role: Accessible.Button; Accessible.name: "Three" }
        }
        """
      And the inspector is connected
      When the agent requests a snapshot
      Then the snapshot is:
        """text
        - window [e1] 400x400
          - button "Three" [e2]
          - (hidden: 3 nodes)
        """

    Scenario: A name is cut after 200 characters
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          objectName: "long"
          width: 80; height: 30
          Accessible.role: Accessible.Button
          Accessible.name: "x".repeat(500)
        }
        """
      And the inspector is connected
      When the agent requests a snapshot
      Then the snapshot matches "x{200}…\" \[e2\]"
      And the snapshot does not match "x{201}"

    Scenario: A name with a line break or a quote stays on its own line
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          width: 80; height: 30
          Accessible.role: Accessible.Button
          Accessible.name: "Save\n- button \"Delete\" [e9]"
        }
        """
      And the inspector is connected
      When the agent requests a snapshot
      Then the snapshot is:
        """text
        - window [e1] 400x400
          - button "Save\n- button \"Delete\" [e9]" [e2]
        """

  Rule: What is typed into a password field is never shown

    Scenario: A password field never reveals its text
      Given the file "Login.qml":
        """qml
        import QtQuick
        TextInput { objectName: "pw"; echoMode: TextInput.Password; text: "hunter2" }
        """
      And the inspector is connected
      When "Login.qml" is loaded
      And the agent requests a snapshot
      Then the snapshot does not contain "hunter2"

    Scenario: A password field is listed with its value masked
      Given the QML:
        """qml
        import QtQuick
        TextInput {
          objectName: "pw"
          width: 120; height: 20
          echoMode: TextInput.Password
          text: "hunter2"
          Accessible.name: "Password"
        }
        """
      And the inspector is connected
      When the agent requests a snapshot
      Then the snapshot contains "\"Password\" value=<redacted> [e2] obj=pw"
      And the snapshot does not contain "hunter2"
      And the snapshot does not contain "•"

    Scenario: A password cannot be found by guessing it
      Given the QML:
        """qml
        import QtQuick
        TextInput {
          width: 120; height: 20
          echoMode: TextInput.Password
          text: "hunter2"
          Accessible.name: "Password"
        }
        """
      And the inspector is connected
      When the agent searches for "text=hunter2"
      Then the search finds 0 nodes

    Scenario: Looking closely at a password field does not reveal it either
      Given the QML:
        """qml
        import QtQuick
        TextInput {
          objectName: "pw"
          width: 120; height: 20
          echoMode: TextInput.Password
          text: "hunter2"
        }
        """
      And the inspector is connected
      When the agent inspects "#pw"
      Then the inspected property "text" has the value "<redacted>"
      And the inspected property "displayText" has the value "<redacted>"
      And the answer does not contain "hunter2"

    Scenario: An inspector started to mask every editable value shows none
      Given the QML:
        """qml
        import QtQuick
        TextInput {
          objectName: "note"
          width: 120; height: 20
          text: "private note"
          Accessible.name: "Note"
        }
        """
      And the inspector is connected and masks every editable value
      When the agent requests a snapshot
      Then the snapshot contains "\"Note\" value=<redacted> [e2] obj=note"
      And the snapshot does not contain "private note"
