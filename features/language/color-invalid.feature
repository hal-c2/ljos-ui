# Sources:
#   https://doc.qt.io/qt-6/qml-color.html
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#colorEqual-method
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#alpha-method
#   https://doc.qt.io/qt-6/qcolor.html#isValid
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A colour that was never given
  A colour property that nothing set holds an invalid colour. It reads as opaque black, but it
  is not valid and it equals no colour that is, transparent black included.

  Rule: An invalid colour reads as black, and says it is not valid

    Scenario Outline: <expression> is <result>
      Given the QML:
        """qml
        import QtQuick
        QtObject {
          objectName: "root"
          property color unset
          property color clear: "transparent"
          property color red: "red"
          property var result: <expression>
        }
        """
      Then the property "result" of "root" is <result>

      Examples: the colour itself
        | expression                       | result      |
        | unset.valid                      | false       |
        | String(unset)                    | "#000000"   |
        | unset.toString()                 | "#000000"   |
        | [unset.r, unset.g, unset.b, unset.a] | [0, 0, 0, 1] |
        | [unset.hslHue, unset.hsvHue, unset.hsvValue] | [0, 0, 0] |
        | typeof unset                     | "object"    |
        | unset == "#000000"               | true        |

      Examples: what is valid
        | expression                       | result      |
        | clear.valid                      | true        |
        | Qt.rgba(0, 0, 0, 0).valid        | true        |
        | Qt.hsla(0, 0, 0, 0).valid        | true        |
        | String(Qt.rgba(0, 0, 0, 0))      | "#00000000" |

      Examples: comparing
        | expression                       | result      |
        | Qt.colorEqual(unset, clear)      | false       |
        | Qt.colorEqual(unset, unset)      | true        |
        | Qt.colorEqual(unset, "#000000")  | false       |
        | Qt.colorEqual(unset, "black")    | false       |

      Examples: made from it
        | expression                       | result      |
        | Qt.lighter(unset).valid          | false       |
        | Qt.darker(unset).valid           | false       |
        | Qt.tint(unset, red).valid        | true        |
        | Qt.tint(red, unset).valid        | false       |
        | Qt.alpha(unset, 0.5).valid       | false       |
        | String(Qt.alpha(unset, 0.5))     | "#80000000" |

  Rule: Setting it makes it valid, and "" unsets it

    Scenario: A channel written makes the colour valid
      Given the QML:
        """qml
        import QtQuick
        QtObject {
          objectName: "root"
          property color unset
          property bool valid: unset.valid
          function write() { unset.r = 1 }
        }
        """
      When the method "write" of "root" is called
      Then the property "unset" of "root" is "#ff0000"
      And the property "valid" of "root" is true

    Scenario: An empty name unsets a colour
      Given the QML:
        """qml
        import QtQuick
        QtObject {
          objectName: "root"
          property color shade: "red"
          property bool held: shade.valid
          function clear() { shade = "" }
        }
        """
      When the method "clear" of "root" is called
      Then the property "held" of "root" is false
      And the property "shade" of "root" is "#000000"

    Scenario: An invalid colour held in a var stays invalid
      Given the QML:
        """qml
        import QtQuick
        QtObject {
          objectName: "root"
          property color unset
          property var kept: unset
          property bool valid: kept.valid
        }
        """
      Then the property "valid" of "root" is false
