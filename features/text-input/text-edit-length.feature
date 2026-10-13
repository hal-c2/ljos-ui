# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html
#   https://doc.qt.io/qt-6/qml-qtquick-textinput.html#maximumLength-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A TextEdit holds text of any length
  TextInput's maximumLength, 32767 unless set, is TextInput's own: a
  TextEdit has no such property and keeps all of a long text, plain or
  rich, and takes more of it.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        width: 300; height: 200
        TextEdit { objectName: "plain"; focus: true; text: "abcdefghij".repeat(4000) }
        TextEdit {
          objectName: "rich"; textFormat: TextEdit.RichText
          text: "<b>abcdefghij</b>".repeat(4000)
        }
      }
      """

  Scenario Outline: A text longer than 32767 characters is kept
    Then the property "length" of "<edit>" is 40000

    Examples:
      | edit  |
      | plain |
      | rich  |

  Scenario Outline: A long text takes more from a program
    When the method "insert" of "<edit>" is called with 40000, "xyz"
    Then the property "length" of "<edit>" is 40003

    Examples:
      | edit  |
      | plain |
      | rich  |

  Scenario: A long text takes more from the user
    When the property "cursorPosition" of "plain" is set to 40000
    And the user types "xyz"
    Then the property "length" of "plain" is 40003
