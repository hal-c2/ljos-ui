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
Feature: A world of falling bodies
  A PhysicsWorld moves the DynamicRigidBody nodes inside its scene. Gravity
  pulls them, shapes stop them, and the world writes where each one is back
  to its position and rotation, so a binding on them follows it.

  Rule: A body falls and comes to rest

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
              objectName: "box"
              y: 200
              collisionShapes: BoxShape {}
              property real doubled: y * 2
            }
          }
        }
        """

    Scenario: A body falls
      When 200 milliseconds pass
      Then the property "y" of "box" is at most 195

    Scenario: A body lands on a plane and rests on it
      When 3 seconds pass
      Then the property "y" of "box" is within 2 of 50
      And the property "isSleeping" of "box" is true

    Scenario: A binding on the position follows the body
      When 200 milliseconds pass
      Then the property "doubled" of "box" is at most 390

    Scenario: A world that is not running moves nothing
      When the property "running" of "world" is set to false
      And 500 milliseconds pass
      Then the property "y" of "box" is within 1 of 200

    Scenario: A body rises when gravity turns the other way
      When 3 seconds pass
      And the property "gravity" of "world" is bound to "Qt.vector3d(0, 981, 0)"
      And 500 milliseconds pass
      Then the property "y" of "box" is at least 100

  Rule: The properties of a world

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

          }
        }
        """

    Scenario Outline: A world starts as the documentation says
      Then the property "<name>" of "world" is <value>

      Examples:
        | name            | value  |
        | running         | true   |
        | typicalLength   | 100    |
        | typicalSpeed    | 1000   |
        | defaultDensity  | 0.001  |
        | minimumTimestep | 1      |
        | numThreads      | -1     |
        | enableCCD       | false  |
        | forceDebugDraw  | false  |

    Scenario: A world says how long its steps were
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        import QtQuick3D.Physics
        Item {
          View3D {
            id: view
            PhysicsWorld {
              objectName: "timed"; scene: view.scene
              property real last: 0
              onFrameDone: (ms) => last = ms
            }
            DynamicRigidBody { collisionShapes: SphereShape {} }
          }
        }
        """
      When 200 milliseconds pass
      Then the property "last" of "timed" is at least 1

    Scenario: A handler that names no parameter reads the step as timestep
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        import QtQuick3D.Physics
        Item {
          View3D {
            id: view
            PhysicsWorld {
              objectName: "timed"; scene: view.scene
              property real last: 0
              onFrameDone: last = timestep
            }
            DynamicRigidBody { collisionShapes: SphereShape {} }
          }
        }
        """
      When 200 milliseconds pass
      Then the property "last" of "timed" is at least 1

  Rule: Which bodies are in a world

    Scenario: A body outside the scene does not fall
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        import QtQuick3D.Physics
        Item {
          View3D {
            Node {
              id: inside
              PhysicsWorld { scene: inside }
            }
            DynamicRigidBody {
              objectName: "box"; y: 200
              collisionShapes: BoxShape {}
            }
          }
        }
        """
      When 300 milliseconds pass
      Then the property "y" of "box" is 200

    Scenario: A node made in the view's scene at run time is drawn from there
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Item {
          width: 100; height: 100
          View3D {
            id: view
            anchors.fill: parent
            PerspectiveCamera { z: 300 }
            DirectionalLight {}
            environment: SceneEnvironment {
              backgroundMode: SceneEnvironment.Color
              clearColor: "black"
            }
          }
          Component {
            id: made
            Model {
              source: "#Cube"
              materials: PrincipledMaterial { lighting: PrincipledMaterial.NoLighting; baseColor: "white" }
            }
          }
          Component.onCompleted: made.createObject(view.scene)
        }
        """
      When 100 milliseconds pass
      Then the pixel at 50, 50 of the window is "#ffffff"
