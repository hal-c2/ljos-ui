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
Feature: The shapes of bodies
  What a body meets is the shape it was given, as big as the scale of its
  node makes it. A shape read from a file is there before anything falls.

  Scenario Outline: A body rests at the height its shape reaches down
    Given the model file "cube.mesh"
    And the QML:
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
            objectName: "body"; y: 200; scale: Qt.vector3d(2, 2, 2)
            physicsMaterial: PhysicsMaterial { restitution: 0 }
            collisionShapes: <shape>
          }
        }
      }
      """
    When 3 seconds pass
    Then the property "y" of "body" is within 3 of <rests>

    Examples:
      | shape                                                  | rests |
      | BoxShape { extents: Qt.vector3d(20, 20, 20) }          | 20    |
      | SphereShape { diameter: 20 }                           | 20    |
      | ConvexMeshShape { source: "cube.mesh" }                | 100   |

  Scenario: A body rests on a mesh of triangles from a file
    Given the model file "cube.mesh"
    And the QML:
      """qml
      import QtQuick
      import QtQuick3D
      import QtQuick3D.Physics
      Item {
        View3D {
          id: view
          PhysicsWorld { scene: view.scene }
          StaticRigidBody { collisionShapes: TriangleMeshShape { source: "cube.mesh" } }
          DynamicRigidBody {
            objectName: "ball"; y: 300
            physicsMaterial: PhysicsMaterial { restitution: 0 }
            collisionShapes: SphereShape { diameter: 20 }
          }
        }
      }
      """
    When 3 seconds pass
    Then the property "y" of "ball" is within 3 of 60

  Scenario: A mesh that cannot be read is said to be
    Given the QML:
      """qml
      import QtQuick
      import QtQuick3D
      import QtQuick3D.Physics
      Item {
        View3D {
          id: view
          PhysicsWorld { scene: view.scene }
          StaticRigidBody { collisionShapes: TriangleMeshShape { source: "missing.mesh" } }
        }
      }
      """
    When 200 milliseconds pass
    Then the log has a warning matching "missing.mesh"

  Scenario: A shape that changes takes effect
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
            objectName: "box"; y: 100
            physicsMaterial: PhysicsMaterial { restitution: 0 }
            collisionShapes: BoxShape { objectName: "shape"; extents: Qt.vector3d(20, 20, 20) }
          }
        }
      }
      """
    When 2 seconds pass
    And the property "extents" of "shape" is bound to "Qt.vector3d(100, 100, 100)"
    And 2 seconds pass
    Then the property "y" of "box" is within 3 of 50
