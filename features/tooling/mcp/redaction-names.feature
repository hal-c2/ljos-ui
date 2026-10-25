# Sources:
#   docs/ARCHITECTURE.md sections 19.4 and 19.7
#   https://doc.qt.io/qt-6/qml-qtquick-textinput.html#echoMode-prop
#   https://doc.qt.io/qt-6/qml-qtqml-qtobject.html#objectName-prop
# The inspector is a Ljós extension: Qt has nothing to compare it with.

@ljos-only @headless
Feature: What names an object does not tell a secret
  A window's title, an objectName and an id say which object is which, and
  each may be made from what a field hides. Wherever the inspector names
  an object (the outline, the status, the list of objects, a search, a
  close look) the secret in a name is masked, both in the text an agent
  reads and in the JSON.

  The window the inspector looks at in a scenario is the test's own, so
  that its title is the test's; that a title made from a secret is told
  masked is held by the inspector's Rust tests, which look at the
  application's own window.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"; width: 300; height: 200
        TextInput {
          id: pw; objectName: "pw"; width: 120; height: 20
          echoMode: TextInput.Password; text: "hunter2"
        }
        Rectangle {
          id: hunter2Box; objectName: "box-" + pw.text
          y: 40; width: 80; height: 30
          Accessible.role: Accessible.Button; Accessible.name: "Box"
        }
      }
      """
    And the inspector is connected

  Rule: A name made from a secret is told with the secret masked

    Scenario: The outline masks an objectName and an id
      When the agent requests a snapshot
      Then the snapshot contains "id=\"<redacted>Box\" obj=\"box-<redacted>\""
      And the snapshot does not contain "hunter2"

    Scenario Outline: <tool> names the objects without the secret
      When the agent calls the tool "<tool>" with:
        """json
        <args>
        """
      Then the answer does not contain "hunter2"

      Examples:
        | tool        | args                         |
        | app_status  | {}                           |
        | app_objects | {}                           |
        | app_find    | {"locator": "role=button"}   |
        | app_inspect | {"target": "role=button"}    |
        | app_inspect | {"target": "#root"}          |
