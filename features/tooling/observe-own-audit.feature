# Sources:
#   docs/ARCHITECTURE.md (section 19.5, `ljos observe`)
#   docs/design/README.md (the tools, the audit's matrix)
#   https://www.w3.org/TR/WCAG22/#target-size-minimum
# The observer is a Ljós tool: Qt has nothing to compare it with.

@ljos-only @desktop
Feature: Every page of the observer passes its own design audit
  The observer is an application with a window, and the audit it offers
  is held to all of its own design rules, warnings and hints as well as
  errors (how fast this machine ran its turns is no design rule):
  in light and in dark, with more contrast, with text twice as large and
  in a narrow window. Its tables say each row's place, a row nothing
  presses is no target to press, a list with nothing in it says so, and a
  spacer that pushes a row's controls to its end is no separator.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        width: 320; height: 160; color: "white"
        ListView {
          anchors.fill: parent
          model: ListModel {
            objectName: "animals"
            ListElement { name: "ant" }
            ListElement { name: "bee" }
          }
          delegate: Text { text: name }
        }
        Timer { interval: 1000; running: true; repeat: true }
        Component.onCompleted: console.warn("a line the application logged")
      }
      """

  Scenario Outline: The <page> page passes its own audit
    When the application is observed
    And the observer's "role=tab[name=\"<page>\"]" is clicked
    Then the observer's own audit at "scheme=light" finds nothing
    And the observer's own audit at "scheme=dark contrast=high" finds nothing
    And the observer's own audit at "font-scale=2" finds nothing
    And the observer's own audit at "size=760x600 scheme=dark" finds nothing

    Examples:
      | page        |
      | Overview    |
      | Processes   |
      | Mailboxes   |
      | Objects     |
      | Logs        |
      | Trace       |
      | Supervision |
      | Frames      |
      | Memory      |
      | Models      |
      | Wakes       |
      | Components  |
      | Bindings    |
      | Growth      |
      | Audit       |

  Scenario: A thread is told apart from another of its name by its id
    When the application is observed
    And the observer's "role=tab[name=\"Processes\"]" is clicked
    Then the observer shows "tid "

  Scenario: The values of nothing picked say how to pick something
    When the application is observed
    And the observer's "role=tab[name=\"Objects\"]" is clicked
    Then the observer shows "Pick an object in the tree to see its properties"
