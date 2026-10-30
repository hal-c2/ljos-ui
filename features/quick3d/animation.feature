# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-vector3danimation.html
#   https://doc.qt.io/qt-6/qml-qtquick-propertyanimation.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-quaternionanimation.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-node.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation; each
# Rule is tagged as checked against Qt once qml6 has checked it.

Feature: Animating where a node is, how big and how it is turned
  A Vector3dAnimation, or a PropertyAnimation on a vector3d, moves each
  component of the vector on its own. A QuaternionAnimation turns a
  rotation along the shortest arc at an even speed (Slerp), or along the
  straight line between the two, normalized (Nlerp); its ends are
  quaternions, or turns about X, Y and Z in degrees. A NumberAnimation on
  eulerRotation.y turns the node too. Each runs linearly over one second.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick3D
      Item {
        id: root; objectName: "root"
        property bool spin: false
        Node {
          id: a; objectName: "a"
          NumberAnimation on eulerRotation.y { running: root.spin; from: 0; to: 90; duration: 1000 }
        }
        Vector3dAnimation {
          objectName: "move"; target: a; property: "position"; duration: 1000
          from: Qt.vector3d(0, 0, 0); to: Qt.vector3d(100, 200, 0)
        }
        PropertyAnimation { objectName: "grow"; target: a; property: "scale"; to: Qt.vector3d(3, 1, 1); duration: 1000 }
        QuaternionAnimation { objectName: "turn"; target: a; property: "rotation"; fromYRotation: 0; toYRotation: 90; duration: 1000 }
        QuaternionAnimation {
          objectName: "straight"; target: a; property: "rotation"; type: QuaternionAnimation.Nlerp
          fromYRotation: 0; toYRotation: 90; duration: 1000
        }
        QuaternionAnimation {
          objectName: "roll"; target: a; property: "rotation"; duration: 1000
          to: Qt.quaternion(0.7071068, 0, 0, 0.7071068)
        }
      }
      """

  Rule: A vector3d moves component by component

    Scenario: A Vector3dAnimation halfway
      When the method "start" of "move" is called
      And 500 milliseconds pass
      Then the property "position.x" of "a" is within 0.01 of 50
      And the property "position.y" of "a" is within 0.01 of 100
      And the property "x" of "a" is within 0.01 of 50

    Scenario: A PropertyAnimation on a vector3d halfway
      When the method "start" of "grow" is called
      And 500 milliseconds pass
      Then the property "scale.x" of "a" is within 0.01 of 2
      And the property "scale.y" of "a" is within 0.01 of 1

    @ljos-only
    Scenario: The animation stops at its end and leaves nothing scheduled
      When the method "start" of "move" is called
      And 2 seconds pass
      Then the property "position.y" of "a" is within 0.01 of 200
      And the application has nothing scheduled

  Rule: A rotation turns along an arc

    Scenario: Slerp turns at an even speed
      When the method "start" of "turn" is called
      And 250 milliseconds pass
      Then the property "eulerRotation.y" of "a" is within 0.01 of 22.5
      When 250 milliseconds pass
      Then the property "eulerRotation.y" of "a" is within 0.01 of 45

    # A quarter of the way along the straight line between no turn and a
    # quarter turn, normalized, is a turn of 21.6°.
    Scenario: Nlerp takes the straight line
      When the method "start" of "straight" is called
      And 250 milliseconds pass
      Then the property "eulerRotation.y" of "a" is within 0.05 of 21.6

    Scenario: Ends given as quaternions
      When the method "start" of "roll" is called
      And 500 milliseconds pass
      Then the property "eulerRotation.z" of "a" is within 0.01 of 45

    @ljos-only
    Scenario: A rotation animation ends where it was sent
      When the method "start" of "turn" is called
      And 2 seconds pass
      Then the property "eulerRotation.y" of "a" is within 0.01 of 90
      And the application has nothing scheduled

  Rule: An Euler angle animates like any number

    Scenario: A NumberAnimation on eulerRotation.y turns the node
      When the property "spin" of "root" is set to true
      And 500 milliseconds pass
      Then the property "eulerRotation.y" of "a" is within 0.01 of 45
      And the property "forward.x" of "a" is within 0.001 of -0.7071
