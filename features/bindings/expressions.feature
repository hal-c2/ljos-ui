# Sources:
#   https://doc.qt.io/qt-6/qtqml-syntax-propertybinding.html
#   https://doc.qt.io/qt-6/qtqml-javascript-expressions.html
#   https://doc.qt.io/qt-6/qtqml-javascript-hostenvironment.html
#   https://doc.qt.io/qt-6/qtqml-javascript-functionlist.html
#   https://doc.qt.io/qt-6/qml-string.html (arg)
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html (point, size, rect)
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: What a binding computes
  A binding is a JavaScript expression: its operators convert their operands as JavaScript's do,
  whatever the types of the properties it reads.

  Rule: An expression gives what JavaScript gives

    Scenario Outline: <expression> is <text>
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property int n: 7
          property real r: 2.5
          property string s: "12"
          property var nothing: null
          property var missing
          function twice(v) { return v * 2 }
          function pick(c) { return c ? n : s }
          property string text: "[" + (<expression>) + "]"
        }
        """
      Then the property "text" of "root" is "[<text>]"

      Examples: adding joins when either side is a string, and every other operator makes numbers
        | expression  | text                |
        | n + 1       | 8                   |
        | s + 1       | 121                 |
        | n + s       | 712                 |
        | s - 2       | 10                  |
        | s * "2"     | 24                  |
        | +s + n      | 19                  |
        | "a" - 1     | NaN                 |
        | n + true    | 8                   |
        | n + nothing | 7                   |
        | n + missing | NaN                 |
        | s + nothing | 12null              |
        | s + missing | 12undefined         |
        | r + r       | 5                   |
        | 0.1 + 0.2   | 0.30000000000000004 |
        | 1 / 3       | 0.3333333333333333  |
        | 1e21 + 1    | 1e+21               |
        | n % 4       | 3                   |
        | -n % 4      | -3                  |
        | 2 ** 10     | 1024                |
        | n >> 1      | 3                   |
        | -n >>> 28   | 15                  |
        | n & 12      | 4                   |

      Examples: dividing by zero is not an error, and a zero keeps its sign
        | expression    | text      |
        | n / 0         | Infinity  |
        | -n / 0        | -Infinity |
        | 0 / 0         | NaN       |
        | 0 * -n        | 0         |
        | 1 / (0 * -n)  | -Infinity |
        | 1 / (0 * n)   | Infinity  |

      Examples: == converts before it compares and === does not
        | expression          | text  |
        | s == 12             | true  |
        | s === 12            | false |
        | s === "12"          | true  |
        | "" == 0             | true  |
        | nothing == missing  | true  |
        | nothing === missing | false |
        | nothing == 0        | false |
        | NaN == NaN          | false |
        | NaN != NaN          | true  |
        | 0 === -0            | true  |
        | s < 5               | false |
        | s < "5"             | true  |
        | "b" > "a"           | true  |
        | n > missing         | false |
        | n >= nothing        | true  |

      Examples: && and \|\| give one of their operands, and anything can be a condition
        | expression              | text      |
        | n && s                  | 12        |
        | 0 && s                  | 0         |
        | "" \|\| r               | 2.5       |
        | s \|\| r                | 12        |
        | nothing ?? n            | 7         |
        | 0 ?? n                  | 0         |
        | !s                      | false     |
        | !""                     | true      |
        | !!"0"                   | true      |
        | !!NaN                   | false     |
        | n > 5 ? "big" : "small" | big       |
        | nothing ? 1 : 2         | 2         |
        | typeof nothing          | object    |
        | typeof missing          | undefined |
        | typeof (n + s)          | string    |
        | typeof (s * 1)          | number    |

      Examples: a template string writes each value as text
        | expression               | text           |
        | `${n} of ${s}`           | 7 of 12        |
        | `${r * 2}px`             | 5px            |
        | `${nothing}/${missing}`  | null/undefined |
        | `${n > 5}`               | true           |

      Examples: the functions JavaScript and Qt have
        | expression               | text     |
        | Math.max(n, s, r)        | 12       |
        | Math.min()               | Infinity |
        | Math.round(r)            | 3        |
        | Math.round(-r)           | -2       |
        | 1 / Math.round(-0.2)     | -Infinity |
        | Math.floor(-r)           | -3       |
        | Math.sqrt(-1)            | NaN      |
        | Math.pow(2, 0.5)         | 1.4142135623730951 |
        | Number(s) + 1            | 13       |
        | Number("")               | 0        |
        | Number("x")              | NaN      |
        | String(nothing) + 1      | null1    |
        | Boolean("false")         | true     |
        | isNaN(missing)           | true     |
        | r.toFixed(0)             | 3        |
        | (1.005).toFixed(2)       | 1.00     |
        | n.toFixed(2)             | 7.00     |
        | s.toUpperCase() + s.length | 122    |
        | "%1 of %2".arg(n).arg(s) | 7 of 12  |
        | qsTr("Open") + "!"       | Open!    |
        | Qt.point(n, 2).x + 1     | 8        |
        | Qt.size(n, 2).height     | 2        |
        | Qt.rect(1, 2, n, 4).width | 7       |

      Examples: a function of the file is called with what the expression gives it
        | expression        | text |
        | twice(n) + 1      | 15   |
        | twice(s)          | 24   |
        | twice("a")        | NaN  |
        | twice(r) + s      | 512  |
        | pick(nothing) + 1 | 121  |
        | pick(s) + 1       | 8    |

  Rule: A binding follows what it read, and nothing else

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property bool flag: true
          property int a: 1
          property int b: 10
          property int either: flag ? a : b
          property int both: flag && a > 0 ? a + b : b
        }
        """

    Scenario: The branch taken is followed
      When the property "a" of "root" is set to 2
      Then the property "either" of "root" is 2
      And the property "both" of "root" is 12

    Scenario: The other branch is followed once the condition changes
      When the property "flag" of "root" is set to false
      And the property "b" of "root" is set to 20
      Then the property "either" of "root" is 20
      And the property "both" of "root" is 20

    Scenario: The condition is followed again after the branch has changed
      When the property "flag" of "root" is set to false
      And the property "flag" of "root" is set to true
      And the property "a" of "root" is set to 3
      Then the property "either" of "root" is 3
      And the property "both" of "root" is 13
