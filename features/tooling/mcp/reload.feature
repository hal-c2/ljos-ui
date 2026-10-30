# Sources:
#   docs/ARCHITECTURE.md sections 12.1, 19.4, 19.5 and 19.6
#   docs/research/10-mcp.md
#   https://modelcontextprotocol.io/specification/2025-11-25/server/resources
# The inspector is a Ljós extension: Qt has nothing to compare it with.

@ljos-only @headless
Feature: An agent that is told about reloads
  When files are watched, an agent can ask for a reload with the capability
  `mutate`, and every client is told when a reload was applied or failed.
  The transport has no channel the server may speak on by itself, so a
  client learns of a reload from `app_status`, from the resource
  `ljos://app/reload`, or by waiting for the next one in a call that is
  parked until it comes. Nothing polls.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        width: 200; height: 100
        property string title: "first"
      }
      """
    And the inspector is connected with the capabilities "inspect, mutate"
    And the files are watched

  Rule: A reload is told

    Scenario: A reload that was applied is told in the status
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item { objectName: "root"; property string title: "second" }
        """
      And a reload happens
      And the agent calls the tool "app_status"
      Then the answer's "/reload/watched" is true
      And the answer's "/reload/generation" is 1
      And the answer's "/reload/last/outcome" is "applied"

    Scenario: A reload that failed says why and keeps the application
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item { objectName: "root"; property string title: }
        """
      And a reload happens
      And the agent calls the tool "app_status"
      Then the answer's "/reload/generation" is 0
      And the answer's "/reload/last/outcome" is "failed"
      And the answer's "/reload/last/message" matches "Scenario"
      And the property "title" of "root" is "first"

    Scenario: The resource tells the same
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item { objectName: "root"; property string title: "second" }
        """
      And a reload happens
      And the agent reads the resource "ljos://app/reload"
      Then the resource contains "applied"

  Rule: A reload is asked for

    Scenario: The changes that wait for the files to be quiet are applied now
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item { objectName: "root"; property string title: "second" }
        """
      And the agent calls the tool "app_reload"
      Then the answer's "/outcome" is "applied"
      And the answer's "/generation" is 1
      And the property "title" of "root" is "second"

    # The save's own reload can come first: its quiet time ends while the
    # call is on its way, or the agent asks a moment after the file was
    # written. The agent asked about its write, and that write is running.
    Scenario: A reload asked for right after a write answers with that write applied
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item { objectName: "root"; property string title: "second" }
        """
      And a reload happens
      And the agent calls the tool "app_reload"
      Then the answer's "/outcome" is "applied"
      And the answer's "/generation" is 1
      And the property "title" of "root" is "second"

    Scenario: A reload asked for again with nothing written since is unchanged
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item { objectName: "root"; property string title: "second" }
        """
      And the agent calls the tool "app_reload"
      And the agent calls the tool "app_reload"
      Then the answer's "/outcome" is "unchanged"
      And the answer's "/generation" is 1

    Scenario: A reload that fails is told as one
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item { objectName: "root"; property string title: }
        """
      And the agent calls the tool "app_reload"
      Then the answer's "/outcome" is "failed"
      And the answer's "/message" matches "Scenario"
      And the property "title" of "root" is "first"

    Scenario: Files that still do not compile are told as failed, not as unchanged
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item { objectName: "root"; property string title: }
        """
      And the agent calls the tool "app_reload"
      And the agent calls the tool "app_reload"
      Then the answer's "/outcome" is "failed"
      And the answer's "/message" matches "Scenario"

    Scenario: Nothing to reload is told as that
      When the agent calls the tool "app_reload"
      Then the answer's "/outcome" is "unchanged"

  Rule: A client can wait for a reload

    Scenario: A client that has missed a reload is answered at once
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item { objectName: "root"; property string title: "second" }
        """
      And a reload happens
      And the agent calls the tool "app_status" with:
        """json
        {"after_reload": 0, "wait_ms": 5000}
        """
      Then the answer's "/reload/generation" is 1
      And the answer's "/reload/waited" is false

    Scenario: A client that waits in vain is answered when its time is up
      When the agent calls the tool "app_status" with:
        """json
        {"after_reload": 0, "wait_ms": 50}
        """
      Then the answer's "/reload/generation" is 0
      And the answer's "/reload/waited" is true

    Scenario: A time that is too long is refused
      When the agent calls the tool "app_status" with:
        """json
        {"after_reload": 0, "wait_ms": 3600000}
        """
      Then the agent is refused with "invalid_argument"
      And the refusal mentions "wait_ms"
