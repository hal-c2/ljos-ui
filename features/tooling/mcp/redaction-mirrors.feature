# Sources:
#   docs/ARCHITECTURE.md sections 19.4 and 19.7
#   docs/research/10-mcp.md
#   https://doc.qt.io/qt-6/qml-qtquick-textinput.html#echoMode-prop
# The inspector is a Ljós extension: Qt has nothing to compare it with.

@ljos-only @headless
Feature: A secret is masked wherever the application repeats it
  What a field that hides its text holds may turn up elsewhere: inside a
  longer text, in a record or a script's object, as one of its keys. The
  inspector masks it there too, and two secrets that are keys of one
  object stay two keys, so that neither a value nor the fact that the
  secrets differ is lost or told.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"; width: 300; height: 200
        property string shout: pw.text + "!"
        TextInput {
          id: pw; objectName: "pw"; width: 120; height: 20
          echoMode: TextInput.Password; text: "hunter2"
        }
        TextInput {
          id: other; objectName: "other"; y: 30; width: 120; height: 20
          echoMode: TextInput.Password; text: "swordfish"
        }
      }
      """

  Rule: A value is masked inside, and the keys of what a script builds too

    Scenario: A property that repeats a secret within other text is inspected without it
      Given the inspector is connected
      When the agent inspects "#root"
      Then the inspected property "shout" has the value "<redacted>!"
      And the answer does not contain "hunter2"

    Scenario: A script that makes secrets the keys of an object is told each key apart
      Given the inspector is connected with the capabilities "inspect, eval"
      When the agent calls the tool "app_eval" with:
        """json
        {"target": "#root", "source": "({[pw.text]: 1, [other.text]: 2, plain: 3})"}
        """
      Then the answer's "/value/plain" is 3
      And the answer's "/value/<redacted>" is 1
      And the answer's "/value/<redacted>#2" is 2
      And the answer does not contain "hunter2"
      And the answer does not contain "swordfish"
