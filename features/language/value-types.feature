# Sources:
#   https://doc.qt.io/qt-6/qtqml-typesystem-valuetypes.html
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html
#   https://doc.qt.io/qt-6/qml-point.html
#   https://doc.qt.io/qt-6/qml-size.html
#   https://doc.qt.io/qt-6/qml-rect.html
#   https://doc.qt.io/qt-6/qml-vector2d.html
#   https://doc.qt.io/qt-6/qml-vector3d.html
#   https://doc.qt.io/qt-6/qml-vector4d.html
#   https://doc.qt.io/qt-6/qml-quaternion.html
#   https://doc.qt.io/qt-6/qml-font.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Value types in JavaScript
  A point, size, rect, vector, quaternion or font is a value: two of them are equal when they hold
  the same, whichever expression made them. It has members, and writing a member of a property
  changes the property.

  Rule: Two values that hold the same are equal

    Scenario Outline: Two <type> values made alike are equal
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property bool strict: <made> === <made>
          property bool loose: <made> == <made>
          property bool other: <made> == <different>
          property int found: [<different>, <made>].indexOf(<made>)
        }
        """
      Then the property "strict" of "root" is true
      And the property "loose" of "root" is true
      And the property "other" of "root" is false
      And the property "found" of "root" is 1

      Examples:
        | type               | made                                                  | different                                             |
        | point              | Qt.point(1, 2)                                        | Qt.point(1, 3)                                        |
        | size               | Qt.size(3, 4)                                         | Qt.size(4, 4)                                         |
        | rect               | Qt.rect(1, 2, 3, 4)                                   | Qt.rect(1, 2, 3, 5)                                   |
        | vector2d           | Qt.vector2d(1, 2)                                     | Qt.vector2d(2, 2)                                     |
        | vector3d           | Qt.vector3d(1, 2, 3)                                  | Qt.vector3d(1, 2, 4)                                  |
        | vector4d           | Qt.vector4d(1, 2, 3, 4)                               | Qt.vector4d(0, 2, 3, 4)                               |
        | quaternion         | Qt.quaternion(1, 2, 3, 4)                             | Qt.quaternion(1, 2, 3, 5)                             |
        | font               | Qt.font({ family: "Arial" })                          | Qt.font({ family: "Times" })                          |
        | font with features | Qt.font({ family: "Arial", features: { "liga": 0 } }) | Qt.font({ family: "Arial", features: { "kern": 0 } }) |

    Scenario: A value read from a property is equal to one made the same way
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property rect area: Qt.rect(1, 2, 3, 4)
          property var kept: Qt.size(5, 6)
          property bool typed: area === Qt.rect(1, 2, 3, 4)
          property bool untyped: kept === Qt.size(5, 6)
          property bool twice: area === area
        }
        """
      Then the property "typed" of "root" is true
      And the property "untyped" of "root" is true
      And the property "twice" of "root" is true

    Scenario: Values of different types are not equal
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property bool pointAndSize: Qt.point(1, 2) == Qt.size(1, 2)
          property bool pointAndVector: Qt.point(1, 2) == Qt.vector2d(1, 2)
        }
        """
      Then the property "pointAndSize" of "root" is false
      And the property "pointAndVector" of "root" is false

  Rule: A value turns into text that names its type

    Scenario Outline: <expression> as text
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property string text: "" + <expression>
          property string asked: <expression>.toString()
        }
        """
      Then the property "text" of "root" is "<text>"
      And the property "asked" of "root" is "<text>"

      Examples:
        | expression                               | text                                               |
        | Qt.point(1, 2)                           | QPointF(1, 2)                                      |
        | Qt.point(1.5, -0.25)                     | QPointF(1.5, -0.25)                                |
        | Qt.point(1234567.891, 0.000001234)       | QPointF(1.23457e+06, 1.234e-06)                    |
        | Qt.point(0.1 + 0.2, 1 / 3)               | QPointF(0.3, 0.333333)                             |
        | Qt.size(3, 4)                            | QSizeF(3, 4)                                       |
        | Qt.rect(1, 2, 3, 4)                      | QRectF(1, 2, 3, 4)                                 |
        | Qt.rect(0.3, 1e-7, 100000.5, 1e6)        | QRectF(0.3, 1e-07, 100001, 1e+06)                  |
        | Qt.vector2d(1, 2)                        | QVector2D(1, 2)                                    |
        | Qt.vector3d(1, 2, 3)                     | QVector3D(1, 2, 3)                                 |
        | Qt.vector3d(0.1, 1 / 3, 12345678)        | QVector3D(0.1, 0.333333, 1.23457e+07)              |
        | Qt.vector4d(1, 2, 3, 4)                  | QVector4D(1, 2, 3, 4)                              |
        | Qt.quaternion(1, 2, 3, 4)                | QQuaternion(1, 2, 3, 4)                            |
        | Qt.font({ family: "Arial", pixelSize: 12 }) | QFont(Arial,-1,12,5,400,0,0,0,0,0,0,0,0,0,0,1,,0,0) |

    Scenario: A value is equal to its text
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property bool asText: Qt.rect(1, 2, 3, 4) == "QRectF(1, 2, 3, 4)"
          property bool asList: Qt.point(1, 2) == "1,2"
          property string kind: typeof Qt.point(1, 2)
        }
        """
      Then the property "asText" of "root" is true
      And the property "asList" of "root" is false
      And the property "kind" of "root" is "object"

  Rule: A value has the members of its type

    Scenario Outline: The members of <value>
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property var value: <value>
          property string members: Object.keys(value).join()
          property string held: Object.keys(value).map(k => value[k]).join()
        }
        """
      Then the property "members" of "root" is "<members>"
      And the property "held" of "root" is "<held>"

      Examples:
        | value                     | members                                  | held            |
        | Qt.point(1, 2)            | x,y                                      | 1,2             |
        | Qt.size(3, 4)             | width,height                             | 3,4             |
        | Qt.rect(1, 2, 3, 4)       | x,y,width,height,left,right,top,bottom   | 1,2,3,4,1,4,2,6 |
        | Qt.vector2d(1, 2)         | x,y                                      | 1,2             |
        | Qt.vector3d(1, 2, 3)      | x,y,z                                    | 1,2,3           |
        | Qt.vector4d(1, 2, 3, 4)   | x,y,z,w                                  | 1,2,3,4         |
        | Qt.quaternion(1, 2, 3, 4) | scalar,x,y,z                             | 1,2,3,4         |

    Scenario: A vector keeps its numbers in single precision
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property real tenth: Qt.vector3d(0.1, 0, 0).x
          property bool exact: Qt.vector3d(0.1, 0, 0).x === 0.1
          property bool pointExact: Qt.point(0.1, 0).x === 0.1
        }
        """
      Then the property "tenth" of "root" is 0.10000000149011612
      And the property "exact" of "root" is false
      And the property "pointExact" of "root" is true

    Scenario: A font has a family, sizes and a weight
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property font face: Qt.font({ family: "Arial", pixelSize: 12, weight: 600, italic: true })
          property string family: face.family
          property int pixels: face.pixelSize
          property bool bold: face.bold
          property int weight: face.weight
          property bool italic: face.italic
          property bool underline: face.underline
        }
        """
      Then the property "family" of "root" is "Arial"
      And the property "pixels" of "root" is 12
      And the property "bold" of "root" is true
      And the property "weight" of "root" is 600
      And the property "italic" of "root" is true
      And the property "underline" of "root" is false

  Rule: Writing a member of a property changes the property

    Scenario: Writing one member of a rect property
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property rect area: Qt.rect(1, 2, 3, 4)
          property string text: "" + area
          property int changes: 0
          onAreaChanged: changes++
          function move() { changes = 0; area.x = 5 }
          function stay() { area.y = 2 }
        }
        """
      When the method "move" of "root" is called
      Then the property "text" of "root" is "QRectF(5, 2, 3, 4)"
      And the property "changes" of "root" is 1
      When the method "stay" of "root" is called
      Then the property "changes" of "root" is 1

    Scenario Outline: Writing a member of a <type> property
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property <type> value: <first>
          property string text: "" + value
          function change() { root.value.<member> = 9 }
          id: root
        }
        """
      When the method "change" of "root" is called
      Then the property "text" of "root" is "<text>"

      Examples:
        | type       | first                     | member | text                    |
        | point      | Qt.point(1, 2)            | y      | QPointF(1, 9)           |
        | size       | Qt.size(3, 4)             | width  | QSizeF(9, 4)            |
        | rect       | Qt.rect(1, 2, 3, 4)       | height | QRectF(1, 2, 3, 9)      |
        | vector2d   | Qt.vector2d(1, 2)         | x      | QVector2D(9, 2)         |
        | vector3d   | Qt.vector3d(1, 2, 3)      | z      | QVector3D(1, 2, 9)      |
        | vector4d   | Qt.vector4d(1, 2, 3, 4)   | w      | QVector4D(1, 2, 3, 9)   |
        | quaternion | Qt.quaternion(1, 2, 3, 4) | scalar | QQuaternion(9, 2, 3, 4) |

    Scenario: Adding to a member and counting it up
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property point spot: Qt.point(1, 2)
          property string text: "" + spot
          property var seen: []
          function change() {
            spot.x += 10
            seen = [spot.y++, ++spot.y, (spot.x = "3")]
          }
        }
        """
      When the method "change" of "root" is called
      Then the property "text" of "root" is "QPointF(3, 4)"
      And the property "seen" of "root" is [2, 4, "3"]

    Scenario: Writing a member of another object's property
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          Item {
            id: other
            objectName: "other"
            property size extent: Qt.size(3, 4)
            property string text: "" + extent
          }
          function grow() { other.extent.height = 40 }
        }
        """
      When the method "grow" of "root" is called
      Then the property "text" of "other" is "QSizeF(3, 40)"

    Scenario: Writing a member of a font property
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property font face: Qt.font({ family: "Arial", pixelSize: 12 })
          property int weight: face.weight
          property int pixels: face.pixelSize
          function embolden() { face.bold = true; face.pixelSize = 30 }
        }
        """
      When the method "embolden" of "root" is called
      Then the property "weight" of "root" is 700
      And the property "pixels" of "root" is 30

    Scenario: Writing a member takes the binding off the property
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property int wide: 10
          property rect area: Qt.rect(1, 2, wide, 4)
          property string text: "" + area
          function move() { area.x = 50 }
        }
        """
      When the method "move" of "root" is called
      And the property "wide" of "root" is set to 20
      Then the property "text" of "root" is "QRectF(50, 2, 10, 4)"

    Scenario: A value in a variable can be changed member by member
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property string text
          property bool same
          function build() {
            var area = Qt.rect(1, 2, 3, 4)
            area.x = 7
            area.width *= 2
            text = "" + area
            same = area === Qt.rect(7, 2, 6, 4)
          }
        }
        """
      When the method "build" of "root" is called
      Then the property "text" of "root" is "QRectF(7, 2, 6, 4)"
      And the property "same" of "root" is true

    Scenario: A value in a list or in an object can be changed member by member
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property string text
          function build() {
            var list = [Qt.point(1, 2)]
            var holder = { spot: Qt.point(3, 4) }
            list[0].x = 5
            holder.spot.y = 6
            text = list[0] + " " + holder.spot
          }
        }
        """
      When the method "build" of "root" is called
      Then the property "text" of "root" is "QPointF(5, 2) QPointF(3, 6)"

  Rule: Qt makes values from numbers

    Scenario Outline: <call> needs all its numbers
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property var value: <call>
        }
        """
      Then the log has a warning matching "<message>"

      Examples:
        | call                      | message                |
        | Qt.point(1)               | Insufficient arguments |
        | Qt.size()                 | Insufficient arguments |
        | Qt.rect(1, 2, 3)          | Insufficient arguments |
        | Qt.vector2d(1)            | Insufficient arguments |
        | Qt.vector3d(1, 2)         | Insufficient arguments |
        | Qt.vector4d(1, 2, 3)      | Insufficient arguments |
        | Qt.quaternion(1, 2, 3)    | Insufficient arguments |
        | Qt.point(1, 2, 3)         | Too many arguments     |
        | Qt.rect(1, 2, 3, 4, 5)    | Too many arguments     |
        | Qt.font()                 | Insufficient arguments |
        | Qt.font("Arial")          | Qt.font..: Invalid arguments |
        | Qt.font({ colour: 1 })    | no valid font subproperties specified |

    Scenario: Numbers written as text are read as numbers
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property string spot: "" + Qt.point("1", "2.5")
          property string vector: "" + Qt.vector2d("1", "x")
        }
        """
      Then the property "spot" of "root" is "QPointF(1, 2.5)"
      And the property "vector" of "root" is "QVector2D(1, nan)"

  Rule: A property of a value type takes values that can be read as one

    Scenario Outline: Text assigned to a <type> property
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property <type> value
          property string text: "" + value
        }
        """
      When the property "value" of "root" is set to "<written>"
      Then the property "text" of "root" is "<text>"

      Examples:
        | type       | written     | text                    |
        | point      | 7,8         | QPointF(7, 8)           |
        | point      | 1.5,-2      | QPointF(1.5, -2)        |
        | size       | 7x8         | QSizeF(7, 8)            |
        | rect       | 1,2,3x4     | QRectF(1, 2, 3, 4)      |
        | rect       | 1, 2, 3 x 4 | QRectF(1, 2, 3, 4)      |
        | vector2d   | 5,6         | QVector2D(5, 6)         |
        | vector3d   | 7,8,9       | QVector3D(7, 8, 9)      |
        | vector4d   | 5,6,7,8     | QVector4D(5, 6, 7, 8)   |
        | quaternion | 5,6,7,8     | QQuaternion(5, 6, 7, 8) |

    Scenario Outline: A <type> property refuses <written>
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property <type> value
        }
        """
      Then setting the property "value" of "root" to <written> is rejected

      Examples:
        | type     | written |
        | point    | "nope"  |
        | point    | 5       |
        | rect     | "1,2"   |
        | size     | true    |
        | vector3d | 5       |

    Scenario: An object with the right members is taken as the value
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property point spot
          property vector2d vector
          property string text: spot + " " + vector
          function fill() {
            spot = { x: 1, y: 2 }
            vector = Qt.point(7, 8)
          }
        }
        """
      When the method "fill" of "root" is called
      Then the property "text" of "root" is "QPointF(1, 2) QVector2D(7, 8)"

  Rule: Vectors do arithmetic

    Scenario Outline: <expression>
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property string text: "" + <expression>
        }
        """
      Then the property "text" of "root" is "<text>"

      Examples:
        | expression                                                 | text                    |
        | Qt.vector2d(1, 2).plus(Qt.vector2d(3, 4))                  | QVector2D(4, 6)         |
        | Qt.vector2d(1, 2).minus(Qt.vector2d(3, 4))                 | QVector2D(-2, -2)       |
        | Qt.vector2d(1, 2).times(Qt.vector2d(3, 4))                 | QVector2D(3, 8)         |
        | Qt.vector2d(1, 2).times(3)                                 | QVector2D(3, 6)         |
        | Qt.vector2d(1, 2).dotProduct(Qt.vector2d(3, 4))            | 11                      |
        | Qt.vector2d(3, 4).normalized()                             | QVector2D(0.6, 0.8)     |
        | Qt.vector2d(3, 4).length()                                 | 5                       |
        | Qt.vector2d(1, 2).toVector3d()                             | QVector3D(1, 2, 0)      |
        | Qt.vector2d(1, 2).toVector4d()                             | QVector4D(1, 2, 0, 0)   |
        | Qt.vector2d(1, 2).fuzzyEquals(Qt.vector2d(1, 2.1), 0.2)    | true                    |
        | Qt.vector3d(1, 2, 3).plus(Qt.vector3d(1, 1, 1))            | QVector3D(2, 3, 4)      |
        | Qt.vector3d(1, 2, 3).minus(Qt.vector3d(1, 1, 1))           | QVector3D(0, 1, 2)      |
        | Qt.vector3d(1, 2, 3).times(Qt.vector3d(3, 4, 5))           | QVector3D(3, 8, 15)     |
        | Qt.vector3d(1, 2, 3).times(2)                              | QVector3D(2, 4, 6)      |
        | Qt.vector3d(1, 2, 3).dotProduct(Qt.vector3d(1, 1, 1))      | 6                       |
        | Qt.vector3d(1, 0, 0).crossProduct(Qt.vector3d(0, 1, 0))    | QVector3D(0, 0, 1)      |
        | Qt.vector3d(1, 2, 3).normalized()                          | QVector3D(0.267261, 0.534522, 0.801784) |
        | Qt.vector3d(0, 0, 0).normalized()                          | QVector3D(0, 0, 0)      |
        | Qt.vector3d(1, 2, 3).length()                              | 3.741657257080078       |
        | Qt.vector3d(1, 2, 3).toVector2d()                          | QVector2D(1, 2)         |
        | Qt.vector3d(1, 2, 3).toVector4d()                          | QVector4D(1, 2, 3, 0)   |
        | Qt.vector3d(1, 2, 3).fuzzyEquals(Qt.vector3d(1, 2, 3.00001)) | true                  |
        | Qt.vector3d(1, 2, 3).fuzzyEquals(Qt.vector3d(1, 2, 3.0001))  | false                 |
        | Qt.vector4d(1, 2, 3, 4).plus(Qt.vector4d(1, 1, 1, 1))      | QVector4D(2, 3, 4, 5)   |
        | Qt.vector4d(1, 2, 3, 4).minus(Qt.vector4d(1, 1, 1, 1))     | QVector4D(0, 1, 2, 3)   |
        | Qt.vector4d(1, 2, 3, 4).times(Qt.vector4d(2, 2, 2, 2))     | QVector4D(2, 4, 6, 8)   |
        | Qt.vector4d(1, 2, 3, 4).times(2)                           | QVector4D(2, 4, 6, 8)   |
        | Qt.vector4d(1, 2, 3, 4).dotProduct(Qt.vector4d(1, 1, 1, 1)) | 10                     |
        | Qt.vector4d(0, 0, 0, 2).normalized()                       | QVector4D(0, 0, 0, 1)   |
        | Qt.vector4d(0, 3, 4, 0).length()                           | 5                       |
        | Qt.vector4d(1, 2, 3, 4).toVector2d()                       | QVector2D(1, 2)         |
        | Qt.vector4d(1, 2, 3, 4).toVector3d()                       | QVector3D(1, 2, 3)      |
