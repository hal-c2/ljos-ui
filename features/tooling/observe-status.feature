# Sources:
#   docs/ARCHITECTURE.md (section 19.5, `ljos observe`)
# The observer is a Ljós tool: Qt has nothing to compare it with.

@ljos-only @desktop
Feature: The observer keeps an error until what failed succeeds
  An edit the application refused is told along the bottom of the
  observer's window. What the observer asks by itself as the application
  runs, its logs and samples among them, answers all the while and does
  not take that error away.

  Scenario: A refused edit's error stays while the application goes on logging
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        width: 200; height: 100; color: "white"
        property int count: 0
        Timer {
          interval: 250; running: true; repeat: true
          onTriggered: console.log("tick")
        }
      }
      """
    When the application is observed
    And the observer's "text=Objects" is clicked
    And the observer's "role=listitem[name=\"Scenario\"]" is clicked
    And the observer's "role=button[name=\"count\"]" is clicked
    And the observer is given the keys "x Return"
    Then the observer has "text=app_set_property: "
    When 3 seconds pass
    Then the observer has "text=app_set_property: "
