# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-quaternion.html
#   https://doc.qt.io/qt-6/qml-quaternion.html
# The value's methods were measured on Qt 6.11.2 (qml6, offscreen). The
# singleton's come from the documentation: QtQuick3D is not installed beside
# qml6 on the reference machine.

Feature: Turns made by name, and what a quaternion can do
  The `Quaternion` singleton of QtQuick3D makes a turn from Euler angles, an
  axis and an angle, a list of them, or what to look at. A `quaternion` value
  multiplies, adds, inverts and turns vectors.

  Rule: The singleton makes turns

    Scenario Outline: A turn by name
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Item {
          objectName: "root"
          property quaternion q: <expression>
          property real w: q.scalar
          property real a: q.x
          property real b: q.y
          property real c: q.z
        }
        """
      Then the property "w" of "root" is within 0.0001 of <w>
      And the property "a" of "root" is within 0.0001 of <x>
      And the property "b" of "root" is within 0.0001 of <y>
      And the property "c" of "root" is within 0.0001 of <z>

      Examples:
        | expression                                                                | w       | x      | y       | z      |
        | Quaternion.fromEulerAngles(90, 0, 0)                                      | 0.70711 | 0.70711 | 0      | 0      |
        | Quaternion.fromEulerAngles(Qt.vector3d(0, 90, 0))                         | 0.70711 | 0      | 0.70711 | 0      |
        | Quaternion.fromAxisAndAngle(Qt.vector3d(0, 2, 0), 90)                     | 0.70711 | 0      | 0.70711 | 0      |
        | Quaternion.fromAxisAndAngle(0, 0, 1, 180)                                 | 0       | 0      | 0       | 1      |
        | Quaternion.fromAxesAndAngles(Qt.vector3d(1, 0, 0), 90, Qt.vector3d(0, 1, 0), 90) | 0.5 | 0.5 | 0.5 | -0.5 |
        | Quaternion.lookAt(Qt.vector3d(0, 0, 0), Qt.vector3d(0, 0, -10))           | 1       | 0      | 0       | 0      |
        | Quaternion.lookAt(Qt.vector3d(0, 0, 0), Qt.vector3d(0, 0, 10))            | 0       | 0      | 1       | 0      |
        | Quaternion.lookAt(Qt.vector3d(0, 0, 0), Qt.vector3d(-10, 0, 0))           | 0.70711 | 0      | 0.70711 | 0      |

    Scenario: A light turned by the singleton shines that way
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Rectangle {
          width: 200; height: 200; color: "black"
          View3D {
            anchors.fill: parent
            environment: SceneEnvironment { clearColor: "#000000"; backgroundMode: SceneEnvironment.Color }
            OrthographicCamera { z: 600 }
            DirectionalLight { rotation: Quaternion.fromEulerAngles(0, 180, 0); ambientColor: "#000000" }
            Model {
              source: "#Rectangle"
              materials: DefaultMaterial { diffuseColor: "#808080" }
            }
          }
        }
        """
      When 100 milliseconds pass
      Then the pixel at 100, 100 of the window is "#000000"

  @qt-parity
  Rule: A quaternion's methods

    Scenario Outline: A method of the value
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property quaternion a: Qt.quaternion(1, 2, 3, 4)
          property quaternion b: Qt.quaternion(0.5, 0, 1, 0)
          property string got: "" + <expression>
        }
        """
      Then the property "got" of "root" is "<result>"

      Examples:
        | expression                                     | result                                       |
        | a.dotProduct(b)                                | 3.5                                          |
        | a.times(b)                                     | QQuaternion(-2.5, -3, 2.5, 4)                |
        | a.times(2)                                     | QQuaternion(2, 4, 6, 8)                      |
        | a.times(Qt.vector3d(1, 0, 0))                  | QVector3D(-20, 20, 10)                       |
        | a.plus(b)                                      | QQuaternion(1.5, 2, 4, 4)                    |
        | a.minus(b)                                     | QQuaternion(0.5, 2, 2, 4)                    |
        | a.normalized()                                 | QQuaternion(0.182574, 0.365148, 0.547723, 0.730297) |
        | a.inverted()                                   | QQuaternion(0.0333333, -0.0666667, -0.1, -0.133333) |
        | a.conjugated()                                 | QQuaternion(1, -2, -3, -4)                   |
        | a.length()                                     | 5.4772257804870605                           |
        | a.toEulerAngles()                              | QVector3D(-41.8103, 79.6951, 116.565)        |
        | a.toVector4d()                                 | QVector4D(2, 3, 4, 1)                        |
        | a.fuzzyEquals(a)                               | true                                         |
        | a.fuzzyEquals(b)                               | false                                        |
        | a.fuzzyEquals(b, 10)                           | true                                         |
        | Qt.quaternion(0, 0, 0, 0).normalized()         | QQuaternion(0, 0, 0, 0)                      |
        | Qt.quaternion(0, 0, 0, 0).inverted()           | QQuaternion(0, 0, 0, 0)                      |
        | Qt.quaternion(0, 0, 0, 0).toEulerAngles()      | QVector3D(0, 0, 0)                           |
        | Qt.quaternion(0.7071068, 0.7071068, 0, 0).toEulerAngles() | QVector3D(90, 0, 0)               |
