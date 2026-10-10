# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#lighter-method
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#darker-method
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#tint-method
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#alpha-method
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#color-method
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#colorEqual-method
#   https://doc.qt.io/qt-6/qml-color.html
#   https://doc.qt.io/qt-6/qt.html#GlobalColor-enum
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Making one colour from another
  Qt has functions that give a lighter, darker, tinted or more transparent colour, and a colour
  property lets one channel be written.

  Rule: Qt gives a colour made from another

    Scenario Outline: <expression> is <colour>
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          objectName: "root"
          color: <expression>
        }
        """
      Then the property "color" of "root" is "<colour>"

      Examples: lighter multiplies the brightness, by one and a half unless told
        | expression                    | colour    |
        | Qt.lighter("#336699")         | #4c99e5   |
        | Qt.lighter("#336699", 2)      | #88c4ff   |
        | Qt.lighter("#336699", 1.2)    | #3d7ab8   |
        | Qt.lighter("red")             | #ff7f7f   |
        | Qt.lighter("black")           | #000000   |
        | Qt.lighter("white")           | #ffffff   |
        | Qt.lighter("#336699", 0)      | #336699   |
        | Qt.lighter("#80336699")       | #804c99e5 |

      Examples: darker divides the brightness, by two unless told
        | expression                    | colour    |
        | Qt.darker("#336699")          | #19334c   |
        | Qt.darker("#336699", 3)       | #112233   |
        | Qt.darker("red")              | #7f0000   |
        | Qt.darker("white", 1.5)       | #aaaaaa   |
        | Qt.darker("#336699", 0.5)     | #88c4ff   |

      Examples: tint lays a translucent colour over another
        | expression                           | colour    |
        | Qt.tint("lightsteelblue", "#10FF0000") | #b5b8d0 |
        | Qt.tint("#336699", "#80ff0000")      | #99334c   |
        | Qt.tint("#336699", "#ff0000")        | #ff0000   |
        | Qt.tint("#336699", "transparent")    | #336699   |
        | Qt.tint("#80336699", "#80ff0000")    | #c099334c |

      Examples: alpha gives the same colour with another opacity
        | expression                    | colour    |
        | Qt.alpha("red", 0.5)          | #80ff0000 |
        | Qt.alpha("#80336699", 0.25)   | #40336699 |
        | Qt.alpha("red", 7)            | #ff0000   |
        | Qt.alpha("red", -1)           | #00ff0000 |

      Examples: color reads a colour name
        | expression                    | colour    |
        | Qt.color("red")               | #ff0000   |
        | Qt.color("#80336699")         | #80336699 |

    Scenario: A colour made by Qt is a colour, equal to the same made another way
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          objectName: "root"
          color: "#336699"
          property bool same: Qt.lighter(color) === Qt.lighter("#336699")
          property bool named: Qt.color("red") === Qt.rgba(1, 0, 0, 1)
          property int blue: Math.round(Qt.darker(color, 3).b * 255)
        }
        """
      Then the property "same" of "root" is true
      And the property "named" of "root" is true
      And the property "blue" of "root" is 51

    Scenario Outline: <expression> is nothing, because the name is not a colour
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property bool nothing: <expression> === null
        }
        """
      Then the property "nothing" of "root" is true

      Examples:
        | expression                 |
        | Qt.lighter("nope")         |
        | Qt.darker("nope")          |
        | Qt.tint("nope", "red")     |
        | Qt.tint("red", "nope")     |
        | Qt.alpha("nope", 0.5)      |

    Scenario: Qt.color refuses a name that is not a colour
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property string said
          function read() {
            try { Qt.color("nope") } catch (e) { said = e.message }
          }
        }
        """
      When the method "read" of "root" is called
      Then the property "said" of "root" is "\"nope\" is not a valid color name"

  Rule: Qt.colorEqual compares two colours, given as colours or as names

    Scenario Outline: Qt.colorEqual(<one>, <other>) is <same>
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          objectName: "root"
          color: "red"
          property bool same: Qt.colorEqual(<one>, <other>)
        }
        """
      Then the property "same" of "root" is <same>

      Examples:
        | one     | other              | same  |
        | "red"   | "#ff0000"          | true  |
        | color   | "red"              | true  |
        | color   | Qt.rgba(1, 0, 0, 1) | true |
        | "red"   | "#fe0000"          | false |
        | "red"   | "#80ff0000"        | false |

    Scenario Outline: Qt.colorEqual refuses <what>
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property string said
          function compare() {
            try { Qt.colorEqual("red", <other>) } catch (e) { said = e.message }
          }
        }
        """
      When the method "compare" of "root" is called
      Then the property "said" of "root" is "<said>"

      Examples:
        | what                         | other  | said                                |
        | a name that is not a colour  | "nope" | Qt.colorEqual(): Invalid color name |
        | a number                     | 5      | Qt.colorEqual(): Invalid arguments  |

  Rule: The colours Qt names are numbers, not colours

    Scenario: Qt.red is a number
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property string kind: typeof Qt.red
          property var named: [Qt.black, Qt.white, Qt.red, Qt.green, Qt.blue, Qt.transparent]
        }
        """
      Then the property "kind" of "root" is "number"
      And the property "named" of "root" is [2, 3, 7, 8, 9, 19]

  Rule: Writing a channel of a colour property changes the property

    Scenario Outline: Writing <channel> of red gives <colour>
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          objectName: "root"
          color: "red"
          property int changes: 0
          onColorChanged: changes += 1
          function write() { changes = 0; color.<channel> = <value> }
        }
        """
      When the method "write" of "root" is called
      Then the property "color" of "root" is "<colour>"
      And the property "changes" of "root" is <changes>

      Examples:
        | channel      | value | colour    | changes |
        | b            | 1     | #ff00ff   | 1       |
        | a            | 0.5   | #80ff0000 | 1       |
        | r            | 1     | #ff0000   | 0       |
        | g            | "0.5" | #ff8000   | 1       |
        | hsvHue       | 0.5   | #00ffff   | 1       |
        | hsvValue     | 0.5   | #800000   | 1       |
        | hslLightness | 0.25  | #800000   | 1       |
        | hslSaturation | 0.5  | #bf4040   | 1       |

    Scenario: Writing a channel of another object's colour
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          Rectangle { id: swatch; objectName: "swatch"; color: "#102030" }
          function fade() { swatch.color.a = 0.5; swatch.color.r += 0.5 }
        }
        """
      When the method "fade" of "root" is called
      Then the property "color" of "swatch" is "#80902030"
