# Sources:
#   docs/ARCHITECTURE.md sections 19.4 and 19.5
#   https://www.erlang.org/doc/apps/observer/observer_ug.html
# The inspector is a Ljós extension: Qt has nothing to compare it with.

@ljos-only @headless
Feature: Processes that do not fit one answer
  An application with more Timers than one answer holds lists as many as
  fit and counts the rest, so an agent knows to ask for one kind. The
  application's own Timers, animations and supervisors are listed before
  the other threads of the process, and the mailboxes always fit beside
  them. 20480 bytes is half the default budget: the answer is sent twice,
  as data and as text, and each copy keeps to it.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        width: 200; height: 200
        Repeater {
          model: 1000
          Item { Timer { objectName: "one of a crowd of timers with a long name"; interval: 1000 + index; repeat: true; running: true } }
        }
      }
      """
    And the inspector is connected

  Scenario: The Timers past what fits are counted
    When the agent calls the tool "app_processes" with:
      """json
      {"kind": "timer"}
      """
    Then the answer is at most 20480 bytes
    And the answer's "/processes/0/kind" is "timer"
    And the answer's "/processes_left_out" is at least 1
    And the answer's "/next_cursor" is absent

  Scenario: The mailboxes still fit beside processes that do not
    When the agent calls the tool "app_processes" with:
      """json
      {"peek": 20}
      """
    Then the answer is at most 20480 bytes
    And the answer's "/processes_left_out" is at least 1
    And the answer's "/mailboxes/0/name" matches "^[a-z]+$"
    And the answer's "/next_cursor" is absent
