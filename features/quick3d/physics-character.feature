# Sources:
#   https://doc.qt.io/qt-6/qtquick3dphysics-index.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-physics-physicsworld.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-physics-dynamicrigidbody.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-physics-physicsnode.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-physics-triggerbody.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-physics-charactercontroller.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-physics-meshshape.html
# Not yet verified on Qt: QtQuick3D.Physics is not installed beside qml6 on
# the reference machine. The expectations come from the documentation.

@ljos-only
Feature: A character walks where it is told
  A CharacterController moves by its movement and its gravity, stops at what
  is in the way, and says what it touches in collisions.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick3D
      import QtQuick3D.Physics
      Item {
        View3D {
          id: view
          PhysicsWorld { objectName: "world"; scene: view.scene }
          StaticRigidBody {
            objectName: "floor"
            eulerRotation.x: -90
            collisionShapes: PlaneShape {}
          }
          CharacterController {
            objectName: "walker"
            function teleportTo(x, y, z) { teleport(Qt.vector3d(x, y, z)) }
            y: 100
            gravity: Qt.vector3d(0, -981, 0)
            collisionShapes: CapsuleShape { diameter: 40; height: 80 }
          }
          StaticRigidBody {
            objectName: "wall"; x: 150; y: 100
            collisionShapes: BoxShape { extents: Qt.vector3d(100, 400, 400) }
          }
        }
      }
      """

  Scenario: It falls until it stands on something
    When 2 seconds pass
    Then the property "y" of "walker" is within 3 of 60
    And the property "collisions" of "walker" is 4

  Scenario: It walks by its movement
    When 1 second passes
    And the property "movement" of "walker" is bound to "Qt.vector3d(0, 0, 100)"
    And 1 second passes
    Then the property "z" of "walker" is within 20 of 100

  Scenario: It stops at a wall
    When 1 second passes
    And the property "movement" of "walker" is bound to "Qt.vector3d(300, 0, 0)"
    And 2 seconds pass
    Then the property "x" of "walker" is within 2 of 80

  Scenario: Teleport puts it somewhere, whatever is in the way
    When 1 second passes
    And the method "teleportTo" of "walker" is called with -50, 60, 0
    And 200 milliseconds pass
    Then the property "x" of "walker" is within 0.5 of -50

  Scenario: Anything but a capsule is refused
    Given the QML:
      """qml
      import QtQuick
      import QtQuick3D
      import QtQuick3D.Physics
      Item {
        View3D {
          id: view
          PhysicsWorld { scene: view.scene }
          CharacterController { collisionShapes: BoxShape {} }
        }
      }
      """
    When 100 milliseconds pass
    Then the log has a warning matching "collision shape is not a capsule"
