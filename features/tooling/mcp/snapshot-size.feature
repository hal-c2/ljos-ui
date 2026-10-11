# Sources:
#   docs/ARCHITECTURE.md sections 19.4, 19.6 and 19.9
#   docs/research/10-mcp.md
#   https://doc.qt.io/qt-6/qml-qtquick-repeater.html
# The inspector is a Ljós extension: Qt has nothing to compare it with.

@ljos-only @headless
Feature: Inspector snapshot size
  A snapshot never grows past what an agent can read: it is cut after a
  number of nodes and says how to ask for the rest, and an agent can ask for
  a part of the application or only for what changed.

  Rule: A long snapshot is cut and continued

    Background:
      Given the QML:
        """qml
        import QtQuick
        Column {
          objectName: "rows"
          property int count: 30
          Repeater {
            model: rows.count
            Rectangle {
              width: 50; height: 5
              Accessible.role: Accessible.Button
              Accessible.name: "Row " + index
            }
          }
          id: rows
        }
        """
      And the inspector is connected

    Scenario: A snapshot that fits is complete
      When the agent requests a snapshot
      Then the snapshot lists 31 nodes
      And the snapshot is complete

    Scenario: A snapshot is cut after the number of nodes the agent asked for
      When the agent requests a snapshot of at most 10 nodes
      Then the snapshot lists 10 nodes
      And the snapshot is cut short
      And the snapshot has a "button" named "Row 8" with a reference
      And the snapshot has no "button" named "Row 9"

    Scenario: The next part starts where the last one stopped
      Given the agent has requested a snapshot of at most 10 nodes
      When the agent requests the next part of the snapshot
      Then the snapshot lists 10 nodes
      And the snapshot has a "button" named "Row 9" with a reference
      And the snapshot has a "button" named "Row 18" with a reference
      And the snapshot has no "button" named "Row 8"
      And the snapshot is cut short

    Scenario: The next part says what its lines are inside of
      Given the agent has requested a snapshot of at most 10 nodes
      When the agent requests the next part of the snapshot
      Then the snapshot contains "- window [e1] 400x400 (continued)"

    Scenario: The last part is complete
      Given the agent has requested a snapshot of at most 20 nodes
      When the agent requests the next part of the snapshot
      Then the snapshot lists 11 nodes
      And the snapshot is complete

    Scenario: Without a number a snapshot is cut after 400 nodes
      When the property "count" of "rows" is set to 450
      And the agent requests a snapshot
      Then the snapshot lists 400 nodes
      And the snapshot is cut short

  Rule: An agent can ask for a part of the application

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          width: 300; height: 200
          Item {
            objectName: "tools"
            width: 300; height: 40
            Accessible.role: Accessible.ToolBar
            Accessible.name: "Tools"
            Rectangle {
              width: 40; height: 40
              Accessible.role: Accessible.Button
              Accessible.name: "Undo"
            }
          }
          Rectangle {
            y: 50; width: 40; height: 40
            Accessible.role: Accessible.Button
            Accessible.name: "Help"
          }
        }
        """
      And the inspector is connected

    Scenario: A snapshot below one item lists that item and what is in it
      When the agent requests a snapshot below "#tools"
      Then the snapshot is:
        """text
        - toolbar "Tools" [e1] obj=tools
          - button "Undo" [e2]
        """

    Scenario: A snapshot one level deep counts what is further down
      When the agent requests a snapshot 1 level deep
      Then the snapshot is:
        """text
        - window [e1] 400x400
          - toolbar "Tools" [e2] obj=tools
            - (deeper: 1 node)
          - button "Help" [e3]
        """

    Scenario: A snapshot below an item nobody has is refused
      When the agent requests a snapshot below "#nowhere"
      Then the agent is refused with "not_found"

  Rule: An agent can ask for what changed since its last snapshot

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          width: 300; height: 200
          property string label: "Save"
          property bool extra: false
          Rectangle {
            width: 40; height: 40
            Accessible.role: Accessible.Button; Accessible.name: root.label
          }
          Rectangle {
            y: 50; width: 40; height: 40
            visible: !root.extra
            Accessible.role: Accessible.Button; Accessible.name: "Help"
          }
          Loader {
            y: 100
            active: root.extra
            sourceComponent: Rectangle {
              width: 40; height: 40
              Accessible.role: Accessible.Button; Accessible.name: "More"
            }
          }
        }
        """
      And the inspector is connected
      And the agent has requested a snapshot

    Scenario: Nothing changed
      When the agent requests what changed since the last snapshot
      Then the snapshot is:
        """text
        (no changes since snapshot 1)
        """

    Scenario: A new name is one changed line
      When the property "label" of "root" is set to "Done"
      And the agent requests what changed since the last snapshot
      Then the snapshot is:
        """text
        changed button "Done" [e2]
        """

    Scenario: Items that came and went are named
      When the property "extra" of "root" is set to true
      And the agent requests what changed since the last snapshot
      Then the snapshot is:
        """text
        added button "More" [e4] under window [e1]
        removed button "Help" [e3]
        """
