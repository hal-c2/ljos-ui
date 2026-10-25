# Sources:
#   docs/ARCHITECTURE.md sections 19.4 and 19.7
#   https://doc.qt.io/qt-6/qml-qtquick-textinput.html#echoMode-prop
#   https://doc.qt.io/qt-6/qml-qtquick-textinput.html#selectedText-prop
#   https://doc.qt.io/qt-6/qml-qtquick-textinput.html#preeditText-prop
# The inspector is a Ljós extension: Qt has nothing to compare it with.

@ljos-only @headless
Feature: A part of a secret is a secret too
  What is selected in a field that hides its text, and what an input
  method is composing in it, are parts of the secret. A binding that
  copies either elsewhere is masked there as the whole text is.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"; width: 300; height: 200
        property string chosen: pw.selectedText
        property string composing: "now " + pin.preeditText
        TextInput {
          id: pw; objectName: "pw"; width: 120; height: 20
          echoMode: TextInput.Password; text: "hunter2"
        }
        TextInput {
          id: pin; objectName: "pin"; y: 30; width: 120; height: 20
          echoMode: TextInput.Password
        }
      }
      """

  Rule: A selection and what is composed are masked where they are copied

    Scenario: A selection of a secret is inspected masked
      Given the inspector is connected
      When the method "select" of "pw" is called with 0, 6
      And the agent inspects "#root"
      Then the inspected property "chosen" has the value "<redacted>"
      And the answer does not contain "hunter"

    Scenario: A script that reads a selection of a secret is told it masked
      Given the inspector is connected with the capabilities "inspect, eval"
      When the method "select" of "pw" is called with 0, 6
      And the agent calls the tool "app_eval" with:
        """json
        {"target": "#root", "source": "chosen"}
        """
      Then the answer's "/value" is "<redacted>"
      And the answer does not contain "hunter"

    Scenario: What an input method composes in a field that hides it is masked
      Given the inspector is connected
      When the method "forceActiveFocus" of "pin" is called
      And the input method composes "4711"
      And the agent inspects "#root"
      Then the inspected property "composing" has the value "now <redacted>"
      And the answer does not contain "4711"
