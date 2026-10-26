# Sources:
#   docs/ARCHITECTURE.md sections 19.4 and 19.5
# The inspector is a Ljós extension: Qt has nothing to compare it with.

@ljos-only @headless
Feature: What a platform does not measure is said, not given as zero
  Where the platform lists no threads and tells nothing of the process's
  memory or CPU time, the process block names what is missing and the
  answer's text says so, so that a client never shows a 0 it did not
  measure.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        width: 200; height: 200
        Timer { id: tick; interval: 250; repeat: true; running: true }
      }
      """
    And the inspector is connected on a platform that measures no process

  Scenario: The process block names what was not measured
    When the agent calls the tool "app_processes" with:
      """json
      {"kind": "thread"}
      """
    Then the answer's "/process/threads" is null
    And the answer's "/process/not_measured" is ["threads", "rss_bytes", "cpu_us"]
    And the answer's "/process/rss_bytes" is absent
    And the answer's "/process/cpu_us" is absent
    And the answer's "/processes" has 0 entries
    And the answer's text contains "threads, memory, CPU time: not measured on this platform"

  Scenario: Timers are listed as on any platform
    When the agent calls the tool "app_processes" with:
      """json
      {"kind": "timer"}
      """
    Then the answer's "/processes" has 1 entries
    And the answer's "/processes/0/name" is "Timer#tick"
