# Sources:
#   https://doc.qt.io/qt-6/qtqml-javascript-expressions.html
#   https://doc.qt.io/qt-6/qtqml-javascript-hostenvironment.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A string literal that goes on over the end of a line
  In a QML document a quoted string may contain a line break, which
  JavaScript itself does not allow. The break is part of the string, as one
  line feed, and so are the spaces that begin the next line. A script file
  that a document imports is plain JavaScript, where such a string is an
  error.

  Rule: The line break is part of the string

    Scenario Outline: A string in <quotes> quotes in a binding
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property string text: <q>one
            two<q>
          property var lines: text.split("\n")
          property int count: lines.length
          property string first: lines[0]
          property string second: lines[1].trim()
          property bool indented: lines[1].length > 3
        }
        """
      Then the property "count" of "root" is 2
      And the property "first" of "root" is "one"
      And the property "second" of "root" is "two"
      And the property "indented" of "root" is true

      Examples:
        | quotes | q |
        | double | " |
        | single | ' |

    Scenario: A string that is part of a larger expression
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property string text: "a" + "b
          c".replace(/ /g, "") + "d"
        }
        """
      Then the property "text" of "root" is "ab\ncd"

    Scenario: A string in a function and in a handler
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property string made
          property string called: lines()
          function lines() {
            let s = "first
              second";
            return s.replace(/ /g, "") + "!"
          }
          Component.onCompleted: made = "x
            y".replace(/ /g, "")
        }
        """
      Then the property "called" of "root" is "first\nsecond!"
      And the property "made" of "root" is "x\ny"

    Scenario: A backslash before the break still joins the lines
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property string text: "a\
          b".replace(/ /g, "")
        }
        """
      Then the property "text" of "root" is "ab"
