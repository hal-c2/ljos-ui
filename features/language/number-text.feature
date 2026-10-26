# Sources:
#   https://tc39.es/ecma262/#sec-numeric-types-number-tostring (the fewest digits that round-trip)
#   https://doc.qt.io/qt-6/qtqml-javascript-hostenvironment.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A number's text
  JavaScript writes a number with the fewest digits that read back as the same number,
  whether a binding or a function turns it into text.

  Rule: A power of two is written in its fewest digits

    Scenario Outline: <expression> is <text> from a binding and from a function
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property real number: <expression>
          property string bound: String(number)
          property string called
          function shown(v) { return String(v) }
          Component.onCompleted: called = shown(number)
        }
        """
      Then the property "bound" of "root" is "<text>"
      And the property "called" of "root" is "<text>"

      Examples: the double below a power of two is twice as close as the one above
        | expression    | text                    |
        | 2 ** 378      | 6.156563468186638e+113  |
        | 2 ** 89       | 6.189700196426902e+26   |
        | 2 ** -24      | 5.960464477539063e-8    |
        | -(2 ** -1017) | -7.120236347223045e-307 |
        | 2 ** 53       | 9007199254740992        |
        | 0.1 + 0.2     | 0.30000000000000004     |
