# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-textinput.html#validator-prop
#   https://doc.qt.io/qt-6/qml-qtquick-textinput.html#acceptableInput-prop
#   https://doc.qt.io/qt-6/qml-qtquick-intvalidator.html
#   https://doc.qt.io/qt-6/qml-qtquick-doublevalidator.html
#   https://doc.qt.io/qt-6/qml-qtquick-regularexpressionvalidator.html
# Verified on Qt 6.11.2 (qml6, offscreen; key input over its vnc platform).

@qt-parity
Feature: Validators of a TextInput
  A validator refuses typed characters that can never lead to an acceptable
  text, and says when the text is acceptable.

  Rule: IntValidator accepts whole numbers in a range

    Scenario Outline: Typing "<typed>" where <bottom> to <top> is allowed
      Given the QML:
        """qml
        import QtQuick
        Item {
          width: 300; height: 200
          TextInput {
            objectName: "input"; focus: true
            width: 200
            validator: IntValidator { bottom: <bottom>; top: <top> }
          }
        }
        """
      When the user types "<typed>"
      Then the property "text" of "input" is "<text>"
      And the property "acceptableInput" of "input" is <acceptable>

      Examples:
        | bottom | top  | typed       | text       | acceptable |
        | 10     | 99   | 42          | 42         | true       |
        | 10     | 99   | 5           | 5          | false      |
        | 10     | 99   | 100         | 10         | true       |
        | 10     | 99   | 4a2         | 42         | true       |
        | 10     | 99   | 4 2         | 42         | true       |
        | 10     | 99   | 4.2         | 42         | true       |
        | 10     | 99   | -5          | 5          | false      |
        | 10     | 99   | +5          | +5         | false      |
        | 10     | 99   | 05          | 05         | false      |
        | 100    | 900  | 0123        | 0123       | true       |
        | 100    | 900  | 999         | 999        | false      |
        | 100    | 900  | 1234        | 123        | true       |
        | 5      | 20   | 30          | 30         | false      |
        | 5      | 20   | 200         | 20         | true       |
        | -10    | 99   | -           | -          | false      |
        | -10    | 99   | -05         | -05        | true       |
        | -10    | 99   | -11         | -1         | true       |
        | -900   | -100 | 5           | 5          | false      |
        | -900   | -100 | -5          | -5         | false      |
        | -900   | -100 | -950        | -95        | false      |
        | -900   | -100 | +5          | 5          | false      |
        | -900   | -100 | -500        | -500       | true       |

    Scenario Outline: Without a range, "<typed>" becomes "<text>"
      Given the QML:
        """qml
        import QtQuick
        Item {
          width: 300; height: 200
          TextInput {
            objectName: "input"; focus: true
            width: 200
            validator: IntValidator {}
          }
        }
        """
      When the user types "<typed>"
      Then the property "text" of "input" is "<text>"
      And the property "acceptableInput" of "input" is true

      Examples:
        | typed       | text       |
        | 1,000       | 1,000      |
        | 12345678901 | 1234567890 |
        | -42         | -42        |

    Scenario: The application can set a text the validator would refuse
      Given the QML:
        """qml
        import QtQuick
        Item {
          width: 300; height: 200
          TextInput {
            objectName: "input"
            validator: IntValidator { bottom: 10; top: 99 }
            text: "abc"
          }
        }
        """
      Then the property "text" of "input" is "abc"
      And the property "acceptableInput" of "input" is false

    Scenario: Return does not accept an unacceptable text
      Given the QML:
        """qml
        import QtQuick
        Item {
          width: 300; height: 200
          TextInput {
            objectName: "input"; focus: true
            property int accepts: 0
            property int finishes: 0
            validator: IntValidator { bottom: 10; top: 99 }
            onAccepted: accepts++
            onEditingFinished: finishes++
          }
        }
        """
      When the user types "5"
      And the user presses "Return"
      Then the property "accepts" of "input" is 0
      And the property "finishes" of "input" is 0
      When the user types "0"
      And the user presses "Return"
      Then the property "accepts" of "input" is 1
      And the property "finishes" of "input" is 1

  Rule: DoubleValidator accepts numbers with a fraction

    Scenario Outline: Standard notation from <bottom> to <top>: "<typed>"
      Given the QML:
        """qml
        import QtQuick
        Item {
          width: 300; height: 200
          TextInput {
            objectName: "input"; focus: true
            width: 200
            validator: DoubleValidator {
              bottom: <bottom>; top: <top>; decimals: <decimals>
              notation: DoubleValidator.StandardNotation
            }
          }
        }
        """
      When the user types "<typed>"
      Then the property "text" of "input" is "<text>"
      And the property "acceptableInput" of "input" is <acceptable>

      Examples:
        | bottom | top | decimals | typed   | text    | acceptable |
        | 0      | 1   | 2        | 0.5     | 0.5     | true       |
        | 0      | 1   | 2        | .5      | .5      | true       |
        | 0      | 1   | 2        | +.5     | +.5     | true       |
        | 0      | 1   | 2        | 7       | 7       | false      |
        | 0      | 1   | 2        | 1.5     | 1.5     | false      |
        | 0      | 1   | 2        | -1      | 1       | true       |
        | 0      | 1   | 2        | 12      | 1       | true       |
        | 0      | 1   | 2        | 0.555   | 0.55    | true       |
        | 0      | 1   | 2        | 1e      | 1       | true       |
        | 0      | 1   | 2        | 0,5     | 0,5     | false      |
        | -5.5   | 100 | 5        | -5.6    | -5.6    | false      |
        | -5.5   | 100 | 5        | -55     | -55     | false      |
        | -5.5   | 100 | 5        | 100.5   | 100.5   | false      |
        | -5.5   | 100 | 5        | 1000    | 100     | true       |
        | -5.5   | 100 | 5        | 3.14159 | 3.14159 | true       |

    Scenario Outline: Scientific notation from 0 to 1: "<typed>"
      Given the QML:
        """qml
        import QtQuick
        Item {
          width: 300; height: 200
          TextInput {
            objectName: "input"; focus: true
            width: 200
            validator: DoubleValidator { bottom: 0; top: 1; decimals: 2 }
          }
        }
        """
      When the user types "<typed>"
      Then the property "text" of "input" is "<text>"
      And the property "acceptableInput" of "input" is <acceptable>

      Examples:
        | typed | text  | acceptable |
        | 0.5e0 | 0.5e0 | true       |
        | 7     | 7     | false      |
        | 12    | 12    | false      |
        | 1e5   | 1e5   | false      |
        | 1e    | 1e    | false      |
        | 1e-   | 1e-   | false      |
        | 1ee   | 1e    | false      |
        | -1    | 1     | true       |
        | 1x    | 1     | true       |

    Scenario Outline: Without a range, "<typed>" becomes "<text>"
      Given the QML:
        """qml
        import QtQuick
        Item {
          width: 300; height: 200
          TextInput {
            objectName: "input"; focus: true
            width: 200
            validator: DoubleValidator {}
          }
        }
        """
      When the user types "<typed>"
      Then the property "text" of "input" is "<text>"
      And the property "acceptableInput" of "input" is true

      Examples:
        | typed | text |
        | 1.5   | 1.5  |
        | -1e3  | -1e3 |
        | 1.2.3 | 1.23 |
        | 1 2   | 12   |

  Rule: RegularExpressionValidator accepts what matches the whole pattern

    Scenario Outline: Typing "<typed>" where <pattern> is allowed
      Given the QML:
        """qml
        import QtQuick
        Item {
          width: 300; height: 200
          TextInput {
            objectName: "input"; focus: true
            width: 200
            validator: RegularExpressionValidator { regularExpression: <pattern> }
          }
        }
        """
      When the user types "<typed>"
      Then the property "text" of "input" is "<text>"
      And the property "acceptableInput" of "input" is <acceptable>

      Examples:
        | pattern   | typed | text | acceptable |
        | /b+/      | abb   | bb   | true       |
        | /[a-c]+x/ | abdcx | abcx | true       |
        | /[a-c]+x/ | abc   | abc  | false      |
        | /AB+/i    | abbc  | abb  | true       |
        | /[0-9]{3}/ | 12345 | 123 | true       |

    Scenario: A validator without a pattern accepts everything
      Given the QML:
        """qml
        import QtQuick
        Item {
          width: 300; height: 200
          TextInput {
            objectName: "input"; focus: true
            validator: RegularExpressionValidator {}
          }
        }
        """
      When the user types "a1!"
      Then the property "text" of "input" is "a1!"
      And the property "acceptableInput" of "input" is true
