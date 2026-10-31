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
Feature: A world at rest costs nothing
  A world steps only while something in it can move. When everything sleeps
  and nothing was asked, the application has nothing scheduled; a new body,
  a command or a change of gravity starts it again.

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
          DynamicRigidBody {
            objectName: "box"; y: 60
            function impulse(x, y, z) { applyCentralImpulse(Qt.vector3d(x, y, z)) }
            physicsMaterial: PhysicsMaterial { restitution: 0 }
            collisionShapes: BoxShape {}
          }
        }
      }
      """

  Scenario: Nothing is scheduled once everything sleeps
    When 3 seconds pass
    Then the property "isSleeping" of "box" is true
    And the application has nothing scheduled

  Scenario: A world not running schedules nothing
    When the property "running" of "world" is set to false
    And 100 milliseconds pass
    Then the application has nothing scheduled

  Scenario: A command wakes a sleeping body
    When 3 seconds pass
    And the method "impulse" of "box" is called with 0, 500000, 0
    And 100 milliseconds pass
    Then the property "y" of "box" is at least 51

  Scenario: A change of gravity wakes it
    When 3 seconds pass
    And the property "gravity" of "world" is bound to "Qt.vector3d(0, 981, 0)"
    And 500 milliseconds pass
    Then the property "y" of "box" is at least 60

  Scenario: Moving what it rests on wakes it
    When 3 seconds pass
    And the property "y" of "floor" is set to -50
    And 500 milliseconds pass
    Then the property "y" of "box" is at most 30
