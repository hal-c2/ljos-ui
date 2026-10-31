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
Feature: Being told what touches what
  A body with receiveContactReports hears of a touch from a body with
  sendContactReports, once when it begins. A TriggerBody stops nothing and
  tells of what goes in and out of it.

  Rule: Contacts

    Scenario: Touching a body is told to the one that listens
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        import QtQuick3D.Physics
        Item {
          View3D {
            id: view
            PhysicsWorld { scene: view.scene }
            StaticRigidBody {
              objectName: "floor"; eulerRotation.x: -90; sendContactReports: true
              physicsMaterial: PhysicsMaterial { restitution: 0 }
              collisionShapes: PlaneShape {}
            }
            DynamicRigidBody {
              objectName: "box"; y: 200; receiveContactReports: true
              function velocity(x, y, z) { setLinearVelocity(Qt.vector3d(x, y, z)) }
              physicsMaterial: PhysicsMaterial { restitution: 0 }
              collisionShapes: BoxShape {}
              property int touches: 0
              property real up: 0
              onBodyContact: (body, positions, impulses, normals) => {
                touches++; up = normals[0].y
              }
            }
          }
        }
        """
      When 3 seconds pass
      Then the property "touches" of "box" is 1
      And the property "up" of "box" is at least 0.9

    Scenario: A body that does not send is not heard
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        import QtQuick3D.Physics
        Item {
          View3D {
            id: view
            PhysicsWorld { scene: view.scene }
            StaticRigidBody { eulerRotation.x: -90; collisionShapes: PlaneShape {} }
            DynamicRigidBody {
              objectName: "box"; y: 100; receiveContactReports: true
              function velocity(x, y, z) { setLinearVelocity(Qt.vector3d(x, y, z)) }
              collisionShapes: BoxShape {}
              property int touches: 0
              onBodyContact: touches++
            }
          }
        }
        """
      When 2 seconds pass
      Then the property "touches" of "box" is 0

    Scenario: A group that is ignored is passed through
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        import QtQuick3D.Physics
        Item {
          View3D {
            id: view
            PhysicsWorld { scene: view.scene }
            StaticRigidBody {
              eulerRotation.x: -90; filterGroup: 1
              collisionShapes: PlaneShape {}
            }
            DynamicRigidBody {
              objectName: "box"; y: 100; filterIgnoreGroups: 2
              function velocity(x, y, z) { setLinearVelocity(Qt.vector3d(x, y, z)) }
              collisionShapes: SphereShape {}
            }
          }
        }
        """
      When 1 second passes
      Then the property "y" of "box" is at most -100

  Rule: Triggers

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        import QtQuick3D.Physics
        Item {
          View3D {
            id: view
            PhysicsWorld { scene: view.scene; gravity: Qt.vector3d(0, 0, 0) }
            TriggerBody {
              objectName: "zone"
              collisionShapes: BoxShape {}
              property int ins: 0
              property int outs: 0
              onBodyEntered: ins++
              onBodyExited: outs++
            }
            DynamicRigidBody {
              objectName: "box"; x: -200; sendTriggerReports: true; receiveTriggerReports: true
              function velocity(x, y, z) { setLinearVelocity(Qt.vector3d(x, y, z)) }
              collisionShapes: SphereShape { diameter: 10 }
              property int heard: 0
              onEnteredTriggerBody: heard++
            }
          }
        }
        """

    Scenario: A body that goes in is told and counted
      When 50 milliseconds pass
      And the method "velocity" of "box" is called with 400, 0, 0
      And 500 milliseconds pass
      Then the property "ins" of "zone" is 1
      And the property "collisionCount" of "zone" is 1
      And the property "heard" of "box" is 1

    Scenario: A body that comes out again is told and no longer counted
      When 50 milliseconds pass
      And the method "velocity" of "box" is called with 400, 0, 0
      And 1500 milliseconds pass
      Then the property "outs" of "zone" is 1
      And the property "collisionCount" of "zone" is 0
      And the property "x" of "box" is at least 300
