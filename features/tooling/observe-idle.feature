# Sources:
#   docs/ARCHITECTURE.md (principle 6: 0 % CPU when idle) and sections 3.4
#   and 19.5 (`ljos observe`)
#   https://www.erlang.org/doc/apps/observer/observer_ug.html
# The observer is a Ljós tool: Qt has nothing to compare it with.

@ljos-only @desktop
Feature: An observer of an application at rest costs nothing
  `ljos observe` follows an application by asking its inspector to be
  told when something changes. While nothing changes, neither the
  application nor the observer does any work: no pass, no frame, no job,
  no message. The observer is woken by what the application did, by a
  timer someone asked for, or by input, and never to ask again whether
  anything happened.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        width: 200; height: 100; color: "white"
        Text { objectName: "label"; text: "at rest" }
      }
      """

  # Longer than any wait a watch was once cut short at (30 s), so that an
  # observer asking again after one would be seen.
  Scenario: An application at rest and its observer do no work for forty seconds
    When the application is observed
    And 2000 milliseconds pass
    And the work of the application and its observer is counted
    And 40000 milliseconds pass
    Then the application and its observer did no work since it was counted
