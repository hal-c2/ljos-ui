# Sources:
#   docs/ARCHITECTURE.md (section 19.5, `ljos observe`)
# The observer is a Ljós tool: Qt has nothing to compare it with.

@ljos-only @desktop
Feature: The observer keeps an error until what failed succeeds
  An edit the application refused is told along the bottom of the
  observer's window. What the observer asks by itself as the application
  runs, its logs and samples among them, answers all the while and does
  not take that error away. An error of what it asks by itself is over
  once the application answers again.

  Scenario: A refused edit's error stays while the application goes on logging
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        width: 200; height: 100; color: "white"
        property int count: 0
        // Goes on answering: each tick's log is followed by a look the
        // application answers. A second apart, so the failed refresh's
        // error is up long enough to be seen first.
        Timer {
          interval: 1000; running: true; repeat: true
          onTriggered: console.log("tick")
        }
      }
      """
    When the application is observed
    And the observer's "text=Objects" is clicked
    And the observer's "role=listitem[name~=\"^Scenario @\"]" is clicked
    And the observer's "role=button[name=\"count\"]" is clicked
    And the observer is given the keys "x Return"
    Then the observer has "text=app_set_property: "
    When 3 seconds pass
    Then the observer has "text=app_set_property: "

  Scenario: A refused edit's error is over once an edit succeeds
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        width: 200; height: 100; color: "white"
        property int count: 0
      }
      """
    When the application is observed
    And the observer's "text=Objects" is clicked
    And the observer's "role=listitem[name~=\"^Scenario @\"]" is clicked
    And the observer's "role=button[name=\"count\"]" is clicked
    And the observer is given the keys "x Return"
    Then the observer has "text=app_set_property: "
    When the observer's "role=button[name=\"count\"]" is clicked
    And the observer is given the keys "5 Return"
    Then the observer does not have "text=app_set_property: "

  Scenario: A refused edit's error is over once another page is chosen
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        width: 200; height: 100; color: "white"
        property int count: 0
        QtObject { id: other }
      }
      """
    When the application is observed
    And the observer's "text=Objects" is clicked
    And the observer's "role=listitem[name~=\"^Scenario @\"]" is clicked
    And the observer's "role=button[name=\"count\"]" is clicked
    And the observer is given the keys "x Return"
    Then the observer has "text=app_set_property: "
    When the observer's "role=tab[name=\"Logs\"]" is clicked
    Then the observer does not have "text=app_set_property: "

  Scenario: A refused edit's error is over once another object is chosen
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        width: 200; height: 100; color: "white"
        property int count: 0
        QtObject { id: other }
      }
      """
    When the application is observed
    And the observer's "text=Objects" is clicked
    And the observer's "role=listitem[name~=\"^Scenario @\"]" is clicked
    And the observer's "role=button[name=\"count\"]" is clicked
    And the observer is given the keys "x Return"
    Then the observer has "text=app_set_property: "
    When the observer's "role=listitem[name~=\"#other$\"]" is clicked
    Then the observer does not have "text=app_set_property: "

  Scenario: A failed refresh's error is over while the application goes on answering
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        width: 200; height: 100; color: "white"
        // Goes on answering: each tick's log is followed by a look the
        // application answers. A second apart, so the failed refresh's
        // error is up long enough to be seen first.
        Timer {
          interval: 1000; running: true; repeat: true
          onTriggered: console.log("tick")
        }
        Loader {
          id: holder
          sourceComponent: Component {
            Rectangle {
              id: kid
              width: 20; height: 20; color: "red"
              property int turn: 0
              onTurnChanged: holder.active = false
            }
          }
        }
      }
      """
    When the application is observed
    And the observer's "text=Objects" is clicked
    And the observer's "role=listitem[name~=\"#kid$\"]" is clicked
    And the observer's "role=button[name=\"turn\"]" is clicked
    And the observer is given the keys "5 Return"
    Then the observer has "text=app_inspect: "
    When 3 seconds pass
    Then the observer does not have "text=app_inspect: "
