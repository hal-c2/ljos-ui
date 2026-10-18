# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-textinput.html#cursorVisible-prop
#   https://doc.qt.io/qt-6/qml-qtquick-stylehints.html#cursorFlashTime-prop
# Verified on Qt 6.11.2 (qml6, offscreen; pointer input over its vnc
# platform), where the cursor flashes every 1000 ms: shown for 500, hidden
# for 500.

@qt-parity @api-QtQuick.TextInput.cursorVisible
Feature: The cursor of a TextInput blinks
  A focused editor's cursor is shown and hidden in turn, from when it took
  the focus. A key that moves the cursor, or typing, shows it again and
  starts the blink over, so that it is seen where it went. A press of the
  pointer does not, whether or not it moves the cursor: pressing again and
  again neither holds the cursor on nor keeps the window drawing.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        width: 200; height: 60; color: "white"
        TextInput {
          objectName: "input"
          x: 10; y: 10; width: 180; height: 20
          font.family: "Liberation Sans"; font.pixelSize: 14
          text: "hello"
        }
      }
      """
    When the user presses the pointer at 11, 25
    And the user releases the pointer at 11, 25

  Scenario: The cursor is shown at first and hidden half a flash later
    When 100 milliseconds pass
    Then the property "cursorPosition" of "input" is 0
    And the pixel at 10, 25 of the window is within 60 of "#000000"
    When 600 milliseconds pass
    Then the pixel at 10, 25 of the window is "#ffffff"

  Scenario: A press that leaves the cursor where it is does not start the blink over
    When 600 milliseconds pass
    And the user presses the pointer at 11, 25
    And the user releases the pointer at 11, 25
    And 200 milliseconds pass
    Then the property "cursorPosition" of "input" is 0
    And the pixel at 10, 25 of the window is "#ffffff"

  Scenario: A press that moves the cursor does not start the blink over
    When the user presses "End"
    And 600 milliseconds pass
    And the user presses the pointer at 11, 25
    And the user releases the pointer at 11, 25
    And 50 milliseconds pass
    Then the property "cursorPosition" of "input" is 0
    And the pixel at 10, 25 of the window is "#ffffff"
    When 450 milliseconds pass
    Then the pixel at 10, 25 of the window is within 60 of "#000000"

  Scenario: A key that moves the cursor shows it again
    When the user presses "End"
    And 600 milliseconds pass
    And the user presses "Home"
    And 50 milliseconds pass
    Then the property "cursorPosition" of "input" is 0
    And the pixel at 10, 25 of the window is within 60 of "#000000"

  Scenario: Typing shows the cursor again
    When 600 milliseconds pass
    And the user types "x"
    And 50 milliseconds pass
    Then the property "cursorPosition" of "input" is 1
    And the pixel at 17, 25 of the window is within 60 of "#000000"
