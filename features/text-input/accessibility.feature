# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-accessible.html#role-prop
#   https://doc.qt.io/qt-6/qml-qtquick-textinput.html#echoMode-prop
#   https://doc.qt.io/qt-6/qml-qtquick-textinput.html#readOnly-prop
#   docs/ARCHITECTURE.md (section 14)
# The Qt oracle cannot see the accessibility tree through QML alone, so these
# scenarios run on Ljós only.

@ljos-only @a11y
Feature: What an editor tells an assistive technology
  An editor is editable text to a screen reader: it says what it holds and
  what is selected, and it can be asked to select and to change its text.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        width: 300; height: 200
        TextInput {
          objectName: "name"; focus: true
          width: 200
          text: "hello world"
          property IntValidator numbers: IntValidator {}
          Accessible.name: "Name"
        }
        TextInput {
          objectName: "secret"
          y: 30; width: 200
          text: "hunter2"
          echoMode: TextInput.Password
          Accessible.name: "Secret"
        }
        TextEdit {
          objectName: "notes"
          y: 60; width: 200; height: 80
          text: "one\ntwo"
          Accessible.name: "Notes"
        }
      }
      """

  Rule: An editor is editable text

    Scenario: A TextInput is a text input without being told
      Then the object "name" is exposed as a "textinput" named "Name"
      And the property "Accessible.role" of "name" is 42

    Scenario: A TextEdit is a text input of many lines
      Then the object "notes" is exposed as a "multilinetextinput" named "Notes"

    Scenario: A role its author gave it stays
      When the property "Accessible.role" of "name" is set to 41
      Then the object "name" is exposed as a "label" named "Name"

    Scenario: The focused editor is where the screen reader is
      Then the accessibility tree's focus is the "textinput" named "Name"

  Rule: It says what it holds

    Scenario: The value is the text
      Then the "textinput" named "Name" has the value "hello world"

    Scenario: The value of a TextEdit is its text
      When the property "text" of "notes" is set to "more"
      Then the "multilinetextinput" named "Notes" has the value "more"

    Scenario: The value follows what is typed
      Given an assistive technology is listening
      When the user presses "End"
      And the user types "!"
      Then the "textinput" named "Name" has the value "hello world!"

    Scenario: What is selected is said
      Given an assistive technology is listening
      When the method "select" of "name" is called with 6, 11
      Then the "textinput" named "Name" has "world" selected

    Scenario: With nothing selected, nothing is said to be
      Then the "textinput" named "Name" has "" selected

  Rule: A field that hides its text never tells it

    Scenario: A password field is one to a screen reader
      Then the object "secret" is exposed as a "passwordinput" named "Secret"

    Scenario: A password field shows a screen reader what it shows the eye
      Then the "passwordinput" named "Secret" has the value "•••••••"

    Scenario: A selection in a password field is of the mask
      Given an assistive technology is listening
      When the method "select" of "secret" is called with 0, 3
      Then the "passwordinput" named "Secret" has "•••" selected

    Scenario: A field that echoes nothing has no value
      When the property "echoMode" of "secret" is set to 1
      Then the "passwordinput" named "Secret" has the value ""

    Scenario: A field typed into in PasswordEchoOnEdit still tells the mask
      Given an assistive technology is listening
      When the property "echoMode" of "secret" is set to 3
      And the property "focus" of "secret" is set to true
      And the user types "ab"
      Then the property "displayText" of "secret" is "ab"
      And the "passwordinput" named "Secret" has the value "••"

  Rule: It does what a screen reader asks

    Scenario: A selection is taken
      When an assistive technology selects characters 0 to 5 of the "textinput" named "Name"
      Then the property "selectedText" of "name" is "hello"
      And the property "cursorPosition" of "name" is 5

    Scenario: A selection may run backwards
      When an assistive technology selects characters 5 to 0 of the "textinput" named "Name"
      Then the property "selectedText" of "name" is "hello"
      And the property "cursorPosition" of "name" is 0

    Scenario: A selection across lines counts the line break
      When an assistive technology selects characters 2 to 5 of the "multilinetextinput" named "Notes"
      Then the property "selectedText" of "notes" is "e\nt"

    Scenario: The selection is replaced
      When an assistive technology selects characters 0 to 5 of the "textinput" named "Name"
      And an assistive technology replaces the selection of the "textinput" named "Name" with "goodbye"
      Then the property "text" of "name" is "goodbye world"

    Scenario: A replacement can be undone like typing
      When an assistive technology selects characters 0 to 5 of the "textinput" named "Name"
      And an assistive technology replaces the selection of the "textinput" named "Name" with "goodbye"
      And the method "undo" of "name" is called
      Then the property "text" of "name" is "hello world"

    Scenario: The whole text is set
      When an assistive technology sets the text of the "textinput" named "Name" to "new"
      Then the property "text" of "name" is "new"
      And the property "cursorPosition" of "name" is 3

    Scenario: A validator judges what a screen reader types
      When the property "text" of "name" is set to "12"
      And the property "validator" of "name" is bound to "numbers"
      And an assistive technology sets the text of the "textinput" named "Name" to "x"
      Then the property "text" of "name" is "12"

  Rule: A read-only editor can be read and not written

    Scenario: It still takes a selection
      When the property "readOnly" of "name" is set to true
      And an assistive technology selects characters 0 to 5 of the "textinput" named "Name"
      Then the property "selectedText" of "name" is "hello"

    Scenario: It takes no text
      When the property "readOnly" of "name" is set to true
      And an assistive technology sets the text of the "textinput" named "Name" to "new"
      And an assistive technology replaces the selection of the "textinput" named "Name" with "x"
      Then the property "text" of "name" is "hello world"

    Scenario: A disabled editor takes nothing
      When the property "enabled" of "name" is set to false
      And an assistive technology selects characters 0 to 5 of the "textinput" named "Name"
      Then the property "selectedText" of "name" is ""
