# Sources:
#   docs/ARCHITECTURE.md sections 3.3, 3.4, 19.4 and 19.5
#   https://www.erlang.org/doc/apps/observer/observer_ug.html
# The inspector is a Ljós extension: Qt has nothing to compare it with.

@ljos-only @headless
Feature: Processes and mailboxes from the inspector
  Like Erlang's observer, an agent or the `ljos observe` window can see
  what does work in the application and the bounded mailboxes work
  reaches the UI thread through, without ever seeing a message's contents.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        width: 200; height: 200
        Timer { id: tick; interval: 250; repeat: true; running: true }
        Timer { id: later; interval: 5000; running: false }
        Rectangle {
          width: 10; height: 10
          NumberAnimation on x { id: slide; from: 0; to: 100; duration: 1000; loops: Animation.Infinite }
        }
      }
      """
    And the inspector is connected

  Rule: The processes are the threads, the Timers and the animations that run

    Scenario: A running Timer is listed with its interval, a stopped one is not
      When the agent calls the tool "app_processes" with:
        """json
        {"kind": "timer"}
        """
      Then the answer's "/processes" has 1 entries
      And the answer's "/processes/0/name" is "Timer#tick"
      And the answer's "/processes/0/locator" is "id=tick"
      And the answer's "/processes/0/interval_ms" is 250
      And the answer's "/processes/0/repeat" is true
      And the answer's "/processes/0/state" is "waiting"

    Scenario: A running animation is listed
      When the agent calls the tool "app_processes" with:
        """json
        {"kind": "animation"}
        """
      Then the answer's "/processes" has 1 entries
      And the answer's "/processes/0/name" is "NumberAnimation#slide"
      And the answer's "/processes/0/state" is "running"

    Scenario: A kind nobody has is refused
      When the agent calls the tool "app_processes" with:
        """json
        {"kind": "fiber"}
        """
      Then the agent is refused with "invalid_argument"

  Rule: A mailbox shows how full it is, never what it holds

    Scenario: The inspector's own inbox is listed with its capacity
      When the agent calls the tool "app_mailboxes" with:
        """json
        {"name": "inspector"}
        """
      Then the answer's "/mailboxes/0/name" is "inspector"
      And the answer's "/mailboxes/0/capacity" is 16
      And the answer's "/counted_since" is "inspector attached"

    Scenario: A peek past the front twenty messages is refused
      When the agent calls the tool "app_mailboxes" with:
        """json
        {"peek": 21}
        """
      Then the agent is refused with "invalid_argument"
