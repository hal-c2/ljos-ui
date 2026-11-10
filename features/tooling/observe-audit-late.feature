# Sources:
#   docs/ARCHITECTURE.md (sections 3.4 and 19.5: `ljos observe` asks the
#   application's inspector, as an agent would)
#   docs/design/README.md (the tools)
# The observer is a Ljós tool: Qt has nothing to compare it with.

@ljos-only @desktop
Feature: The observer shows an audit's findings when its answer comes
  The observer asks the application for its audit and goes on: the answer
  comes when the application has audited what it shows, which takes
  longer the more it shows. Whoever reads the observer's window, an agent
  or a person, sees the findings once they have come, not only when the
  answer beat them to it.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        width: 640; height: 480; color: "white"
        Grid {
          columns: 20
          Repeater {
            model: 600
            Text { text: "faint " + index; color: "#d8d8d8"; font.pixelSize: 10 }
          }
        }
      }
      """

  Scenario: The findings of a long audit are listed when they come
    When the application is observed
    And the observer's "text=Audit" is clicked
    And the observer's "role=button[name=\"Run audit\"]" is clicked
    Then the observer has "text=color-text-contrast"

  Scenario: A finding of a long audit can be picked when it is listed
    When the application is observed
    And the observer's "text=Audit" is clicked
    And the observer's "role=button[name=\"Run audit\"]" is clicked
    And the observer's "role=listitem >> nth=0" is clicked
    Then the observer has "role=button[name=\"Open guide\"]"
