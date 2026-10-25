# Sources:
#   https://doc.qt.io/qt-6/qtqml-typesystem-valuetypes.html
#   https://doc.qt.io/qt-6/qml-string.html
#   https://doc.qt.io/qt-6/qml-list.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: What a value type makes of a value of another type
  A point, size, rect or vector given another of them keeps the parts the two share: a
  rect's corner is a point, its size a size, and a point is a rect with no size. One of them
  given a list, an object or a colour is its empty value, and a colour given what is no
  colour is an invalid one, all without a word; a number or a bool is refused and warned
  about. A string given a list is a character for each item, and a font is Qt's text of it.
  A script's write is converted by the same rules, and throws where a binding warns.

  Scenario Outline: A <type> given <value> reads <seen>
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        function pass(v) { return v }
        property <type> v: pass(<value>)
        property string seen: JSON.stringify(v).replace(/"/g, "'")
      }
      """
    Then the property "seen" of "root" is "<seen>"
    And the log has no warning matching "assign"

    Examples:
      | type     | value                     | seen                                                                                                                            |
      | rect     | Qt.size(3, 4)             | {'x':0,'y':0,'width':3,'height':4,'left':0,'right':3,'top':0,'bottom':4}                                                        |
      | rect     | Qt.vector3d(1, 2, 3)      | {'x':1,'y':2,'width':0,'height':0,'left':1,'right':1,'top':2,'bottom':2}                                                        |
      | rect     | null                      | {'x':0,'y':0,'width':0,'height':0,'left':0,'right':0,'top':0,'bottom':0}                                                        |
      | point    | Qt.rect(1, 2, 3, 4)       | {'x':1,'y':2}                                                                                                                   |
      | point    | Qt.quaternion(1, 2, 3, 4) | {'x':2,'y':3}                                                                                                                   |
      | point    | Qt.size(3, 4)             | {'x':0,'y':0}                                                                                                                   |
      | size     | Qt.rect(1, 2, 3, 4)       | {'width':3,'height':4}                                                                                                          |
      | size     | Qt.point(3, 4)            | {'width':-1,'height':-1}                                                                                                        |
      | size     | ({ width: 3 })            | {'width':3,'height':-1}                                                                                                         |
      | size     | ({})                      | {'width':-1,'height':-1}                                                                                                        |
      | vector2d | Qt.rect(1, 2, 3, 4)       | {'x':1,'y':2}                                                                                                                   |
      | vector2d | "zz"                      | {'x':0,'y':0}                                                                                                                   |
      | vector3d | Qt.point(1, 2)            | {'x':1,'y':2,'z':0}                                                                                                             |
      | vector3d | Qt.vector4d(1, 2, 3, 4)   | {'x':1,'y':2,'z':3}                                                                                                             |
      | color    | ({})                      | {'r':0,'g':0,'b':0,'a':1,'hsvHue':0,'hsvSaturation':0,'hsvValue':0,'hslHue':0,'hslSaturation':0,'hslLightness':0,'valid':false} |
      | color    | Qt.size(1, 2)             | {'r':0,'g':0,'b':0,'a':1,'hsvHue':0,'hsvSaturation':0,'hsvValue':0,'hslHue':0,'hslSaturation':0,'hslLightness':0,'valid':false} |

  Scenario Outline: A <type> given <value> is its empty value and warned about
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        function pass(v) { return v }
        property <type> v: pass(<value>)
        property string seen: JSON.stringify(v).replace(/"/g, "'")
      }
      """
    Then the property "seen" of "root" is "<seen>"
    And the log has a warning matching "Unable to assign <what> to <qt>"

    Examples:
      | type     | value | seen                                                                     | what   | qt        |
      | rect     | 5     | {'x':0,'y':0,'width':0,'height':0,'left':0,'right':0,'top':0,'bottom':0} | int    | QRectF    |
      | point    | true  | {'x':0,'y':0}                                                            | bool   | QPointF   |
      | size     | 2.5   | {'width':-1,'height':-1}                                                 | double | QSizeF    |
      | vector3d | false | {'x':0,'y':0,'z':0}                                                      | bool   | QVector3D |

  Scenario Outline: A string given <value> is the characters <codes>
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        function pass(v) { return v }
        property string s: pass(<value>)
        property string codes: Array.from(s).map(c => c.charCodeAt(0)).join(" ")
      }
      """
    Then the property "codes" of "root" is "<codes>"
    And the log has no warning matching "assign"

    Examples:
      | value                      | codes       |
      | [true, 72]                 | 1 72        |
      | ["", "xy", 72]             | 0 120 72    |
      | [72.9, -1, 65536 + 72]     | 72 65535 72 |
      | [{}, Qt.point(1, 2), null] | 0 0 0       |

  Scenario: A script's write is converted as a binding's value is, and throws where it warns
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property rect r
        property vector2d v: Qt.vector2d(1, 1)
        property string s
        property string t
        property font f
        property string seen
        function t1(write) { try { write(); return "ok" } catch (e) { return e.message } }
        function write() {
          seen = [t1(() => r = Qt.size(1, 2)), t1(() => v = "zz"), t1(() => s = NaN), t1(() => t = Qt.font({ family: "x" })),
                  t1(() => f = "q"), t1(() => f = 5), t1(() => r = 5), r.width, v.x, s, t, f.family].join(";")
        }
      }
      """
    When the method "write" of "root" is called
    Then the property "seen" of "root" is "ok;ok;ok;ok;ok;Cannot assign int to QFont;Cannot assign int to QRectF;1;0;nan;x,9,-1,5,400,0,0,0,0,0,0,0,0,0,0,1,,0,0;q"

  Scenario: A colour or font pushed onto a list of strings is its text
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property list<string> names
        property string seen
        function add() {
          names.push(Qt.rgba(1, 0, 0, 1)); names.push(Qt.font({ family: "x" })); names.push(Qt.point(1, 2))
          seen = names.join("|")
        }
      }
      """
    When the method "add" of "root" is called
    Then the property "seen" of "root" is "#ff0000|x,9,-1,5,400,0,0,0,0,0,0,0,0,0,0,1,,0,0|"
