# Sources:
#   https://doc.qt.io/qt-6/qtqml-syntax-signals.html
#   https://doc.qt.io/qt-6/qtqml-syntax-objectattributes.html#signal-attributes
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: What a signal's typed parameters make of what a script gives them
  A script's emit gives each typed parameter what JavaScript makes of the value: a number
  as Number() does, an int cut to 32 bits, a string as String() does but empty for
  undefined and null, and a bool as Boolean() does. A colour, point, size, rect, font or
  url that can't be made of the value is its empty value, and qml6 says so. Too few
  arguments throw, and no handler runs.

  Scenario Outline: A <type> parameter given <given> reads <seen>
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string seen
        signal s(<type> v)
        onS: function (v) { seen = typeof v + ":" + v }
        function send() { s(<given>) }
      }
      """
    When the method "send" of "root" is called
    Then the property "seen" of "root" is "<seen>"
    And the log has no warning matching "argument"

    Examples:
      | type   | given     | seen                   |
      | real   | undefined | number:NaN             |
      | real   | "x"       | number:NaN             |
      | real   | ({})      | number:NaN             |
      | real   | null      | number:0               |
      | real   | [5]       | number:5               |
      | int    | undefined | number:0               |
      | int    | "x"       | number:0               |
      | int    | 1.75      | number:1               |
      | int    | -1.75     | number:-1              |
      | int    | 3e9       | number:-1294967296     |
      | string | undefined | string:                |
      | string | null      | string:                |
      | string | NaN       | string:NaN             |
      | string | [65, 66]  | string:65,66           |
      | string | ({})      | string:[object Object] |
      | bool   | undefined | boolean:false          |
      | bool   | "x"       | boolean:true           |
      | bool   | ({})      | boolean:true           |
      | bool   | ""        | boolean:false          |

  Scenario Outline: A <type> parameter given <given> is its empty value and warned about
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string seen
        signal s(int first, <type> v)
        onS: function (first, v) { seen = String(<read>) }
        function send() { s(0, <given>) }
      }
      """
    When the method "send" of "root" is called
    Then the property "seen" of "root" is "<seen>"
    And the log has a warning matching "Could not convert argument 1 from <from> to <qt>"
    And the log has a warning matching "Passing incompatible arguments to signals is not supported."

    Examples:
      | type  | given     | read   | seen               | from            | qt      |
      | color | undefined | v      | #000000            | undefined       | QColor  |
      | point | undefined | v      | QPointF(0, 0)      | undefined       | QPointF |
      | point | null      | v      | QPointF(0, 0)      | null            | QPointF |
      | point | 5         | v      | QPointF(0, 0)      | 5               | QPointF |
      | size  | null      | v      | QSizeF(-1, -1)     | null            | QSizeF  |
      | rect  | 5         | v      | QRectF(0, 0, 0, 0) | 5               | QRectF  |
      | font  | null      | v.bold | false              | null            | QFont   |
      | url   | 5         | v      |                    | 5               | QUrl    |
      | url   | null      | v      |                    | null            | QUrl    |
      | url   | ({})      | v      |                    | .object Object. | QUrl    |

  Scenario Outline: A <type> parameter given <given> takes what it can of it
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string seen
        signal s(<type> v)
        onS: function (v) { seen = String(v) }
        function send() { s(<given>) }
      }
      """
    When the method "send" of "root" is called
    Then the property "seen" of "root" is "<seen>"
    And the log has no warning matching "argument"

    Examples:
      | type  | given                    | seen          |
      | point | ({x: 1, y: 2})           | QPointF(1, 2) |
      | point | ({})                     | QPointF(0, 0) |
      | size  | ({width: 3, height: 4})  | QSizeF(3, 4)  |
      | color | "red"                    | #ff0000       |
      | color | null                     | #000000       |
      | color | 5                        | #000000       |
      | color | ({})                     | #000000       |
      | url   | "b.png"                  | b.png         |

  Scenario: A signal given too few arguments throws and tells no handler
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string seen: "none"
        signal s(real a, real b)
        onS: seen = "told"
        function send() { try { s(1) } catch (e) { seen = e.message } }
      }
      """
    When the method "send" of "root" is called
    Then the property "seen" of "root" is "Insufficient arguments"
