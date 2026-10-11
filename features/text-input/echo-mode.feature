# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-textinput.html#echoMode-prop
#   https://doc.qt.io/qt-6/qml-qtquick-textinput.html#passwordCharacter-prop
#   https://doc.qt.io/qt-6/qml-qtquick-textinput.html#displayText-prop
# Verified on Qt 6.11.2 (qml6, offscreen; key input over its vnc platform).

@qt-parity
Feature: Echo modes of a TextInput
  A TextInput can hide what is typed into it, as a password field does.

  Rule: Password shows one mask character for each character

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          width: 300; height: 200
          TextInput {
            objectName: "input"; focus: true
            width: 200
            echoMode: TextInput.Password
            text: "secret key"
          }
        }
        """

    Scenario: The text is kept and the display is masked
      Then the property "text" of "input" is "secret key"
      And the property "displayText" of "input" is "••••••••••"

    Scenario: Typed characters are masked too
      When the user types "s"
      Then the property "displayText" of "input" is "•••••••••••"
      And the property "text" of "input" is "secret keys"

    Scenario: The mask character can be chosen
      When the property "passwordCharacter" of "input" is set to "*"
      Then the property "displayText" of "input" is "**********"

    Scenario: A password cannot be copied
      Given "kept" was copied in another application
      When the user presses "Ctrl+A"
      And the user presses "Ctrl+C"
      Then the clipboard holds "kept"

    Scenario: A password cannot be cut
      Given "kept" was copied in another application
      When the user presses "Ctrl+A"
      And the user presses "Ctrl+X"
      Then the clipboard holds "kept"
      And the property "text" of "input" is "secret key"

    Scenario: The words of a password are not shown by word movement
      When the user presses "Ctrl+Left"
      Then the property "cursorPosition" of "input" is 0

    Scenario: The application still sees the selected text
      When the method "select" of "input" is called with 0, 6
      Then the property "selectedText" of "input" is "secret"

  Rule: NoEcho shows nothing at all

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          width: 300; height: 200
          TextInput {
            objectName: "input"; focus: true
            width: 200
            echoMode: TextInput.NoEcho
          }
        }
        """

    Scenario: Typed text is kept but not displayed
      When the user types "abc"
      Then the property "text" of "input" is "abc"
      And the property "displayText" of "input" is ""
      And the property "contentWidth" of "input" is 0
      And the property "cursorRectangle.x" of "input" is 0

    Scenario: The cursor cannot be moved through what is not shown
      When the user types "abc"
      And the user presses "Left"
      And the user presses "Backspace"
      Then the property "text" of "input" is "ab"
      And the property "cursorPosition" of "input" is 2

  Rule: PasswordEchoOnEdit shows the text only while it is edited

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          width: 300; height: 200
          TextInput {
            objectName: "input"; focus: true
            width: 200
            echoMode: TextInput.PasswordEchoOnEdit
          }
          Item { objectName: "other" }
        }
        """

    Scenario: What is being typed is shown
      When the user types "abc"
      Then the property "displayText" of "input" is "abc"

    Scenario: It is masked when the focus leaves
      When the user types "abc"
      And the method "forceActiveFocus" of "other" is called
      Then the property "displayText" of "input" is "•••"
      And the property "text" of "input" is "abc"

    Scenario: A text that was not typed just now is masked
      When the property "text" of "input" is set to "abc"
      Then the property "displayText" of "input" is "•••"

    Scenario: Editing a masked text starts it over
      When the property "text" of "input" is set to "abc"
      And the user types "x"
      Then the property "text" of "input" is "x"
      And the property "displayText" of "input" is "x"

    Scenario: Coming back to the field starts it over again
      When the user types "ab"
      And the method "forceActiveFocus" of "other" is called
      And the method "forceActiveFocus" of "input" is called
      And the user types "c"
      Then the property "text" of "input" is "c"

    Scenario: Starting over can be undone
      When the property "text" of "input" is set to "abc"
      And the user presses "Left"
      Then the property "text" of "input" is ""
      And the property "canUndo" of "input" is true

    Scenario: Return accepts the masked text as it is
      When the property "text" of "input" is set to "abc"
      And the user presses "Return"
      Then the property "text" of "input" is "abc"
