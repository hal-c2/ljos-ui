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
Feature: Moving a body by hand
  What a DynamicRigidBody is asked to do is done in the next step of its
  world, in the order it was asked. Its weight says how far a push takes it.

  Rule: Forces and impulses move it

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        import QtQuick3D.Physics
        Item {
          View3D {
            id: view
            PhysicsWorld { objectName: "world"; scene: view.scene; gravity: Qt.vector3d(0, 0, 0) }
            DynamicRigidBody {
              objectName: "box"
              property real turned: rotation.y
              function backTo(x, y, z) { reset(Qt.vector3d(x, y, z), Qt.vector3d(0, 0, 0)) }
              function force(x, y, z) { applyCentralForce(Qt.vector3d(x, y, z)) }
              function impulse(x, y, z) { applyCentralImpulse(Qt.vector3d(x, y, z)) }
              function spin(x, y, z) { applyTorqueImpulse(Qt.vector3d(x, y, z)) }
              function velocity(x, y, z) { setLinearVelocity(Qt.vector3d(x, y, z)) }
              massMode: DynamicRigidBody.Mass
              mass: 2
              collisionShapes: SphereShape {}
            }
          }
        }
        """

    Scenario: An impulse divided by the mass gives the speed
      When the method "impulse" of "box" is called with 200, 0, 0
      And 1 second passes
      Then the property "x" of "box" is within 15 of 100

    Scenario: Setting the velocity moves it that fast
      When the method "velocity" of "box" is called with 0, 50, 0
      And 1 second passes
      Then the property "y" of "box" is within 8 of 50

    Scenario: A force acts for one step only
      When the method "force" of "box" is called with 2000, 0, 0
      And 500 milliseconds pass
      Then the property "x" of "box" is at least 0.5
      And the property "x" of "box" is at most 40

    Scenario: A torque impulse turns it
      When the method "spin" of "box" is called with 0, 5000, 0
      And 500 milliseconds pass
      Then the property "turned" of "box" is at least 0.1

    Scenario: Reset puts it back, standing still
      When the method "velocity" of "box" is called with 100, 0, 0
      And 200 milliseconds pass
      And the method "backTo" of "box" is called with 0, 10, 0
      And 500 milliseconds pass
      Then the property "x" of "box" is within 0.1 of 0
      And the property "y" of "box" is within 0.1 of 10

  Rule: What holds a body back

    Scenario: A locked axis does not move
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        import QtQuick3D.Physics
        Item {
          View3D {
            id: view
            PhysicsWorld { scene: view.scene }
            DynamicRigidBody {
              objectName: "box"; y: 200
              function force(x, y, z) { applyCentralForce(Qt.vector3d(x, y, z)) }
              function impulse(x, y, z) { applyCentralImpulse(Qt.vector3d(x, y, z)) }
              function spin(x, y, z) { applyTorqueImpulse(Qt.vector3d(x, y, z)) }
              function velocity(x, y, z) { setLinearVelocity(Qt.vector3d(x, y, z)) }
              linearAxisLock: DynamicRigidBody.LockY
              collisionShapes: SphereShape {}
            }
          }
        }
        """
      When 500 milliseconds pass
      Then the property "y" of "box" is within 0.01 of 200

    Scenario: A body with gravity off does not fall
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        import QtQuick3D.Physics
        Item {
          View3D {
            id: view
            PhysicsWorld { scene: view.scene }
            DynamicRigidBody {
              objectName: "box"; y: 200
              function force(x, y, z) { applyCentralForce(Qt.vector3d(x, y, z)) }
              function impulse(x, y, z) { applyCentralImpulse(Qt.vector3d(x, y, z)) }
              function spin(x, y, z) { applyTorqueImpulse(Qt.vector3d(x, y, z)) }
              function velocity(x, y, z) { setLinearVelocity(Qt.vector3d(x, y, z)) }
              gravityEnabled: false
              collisionShapes: SphereShape {}
            }
          }
        }
        """
      When 500 milliseconds pass
      Then the property "y" of "box" is within 0.01 of 200

  Rule: A kinematic body goes where it is told

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
              objectName: "lift"
              y: 300
              isKinematic: true
              kinematicPosition: Qt.vector3d(0, 300, 0)
              collisionShapes: BoxShape {}
            }
          }
        }
        """

    Scenario: It does not fall
      When 500 milliseconds pass
      Then the property "y" of "lift" is within 0.01 of 300

    Scenario: It moves to its kinematic position
      When the property "kinematicPosition" of "lift" is bound to "Qt.vector3d(40, 300, 0)"
      And 200 milliseconds pass
      Then the property "x" of "lift" is within 0.01 of 40

    Scenario: A mesh of triangles makes a body kinematic, with a warning
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        import QtQuick3D.Physics
        Item {
          View3D {
            id: view
            PhysicsWorld { scene: view.scene }
            DynamicRigidBody {
              objectName: "bad"; y: 100
              collisionShapes: PlaneShape {}
            }
          }
        }
        """
      When 300 milliseconds pass
      Then the log has a warning matching "forcing kinematic"
      And the property "y" of "bad" is within 0.01 of 100

  Rule: Weight

    Scenario Outline: A property gives a body's weight the way its mode says
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        import QtQuick3D.Physics
        Item {
          View3D {
            id: view
            PhysicsWorld { scene: view.scene; gravity: Qt.vector3d(0, 0, 0) }
            DynamicRigidBody {
              objectName: "box"
              function force(x, y, z) { applyCentralForce(Qt.vector3d(x, y, z)) }
              function impulse(x, y, z) { applyCentralImpulse(Qt.vector3d(x, y, z)) }
              function spin(x, y, z) { applyTorqueImpulse(Qt.vector3d(x, y, z)) }
              function velocity(x, y, z) { setLinearVelocity(Qt.vector3d(x, y, z)) }
              massMode: DynamicRigidBody.<mode>
              density: 0.002
              mass: 4
              inertiaTensor: Qt.vector3d(1, 1, 1)
              collisionShapes: BoxShape { extents: Qt.vector3d(10, 10, 10) }
            }
          }
        }
        """
      When 50 milliseconds pass
      And the method "impulse" of "box" is called with <push>, 0, 0
      And 1 second passes
      Then the property "x" of "box" is within 15 of 100

      Examples:
        | mode                 | push |
        | CustomDensity        | 200  |
        | Mass                 | 400  |
        | MassAndInertiaTensor | 400  |
