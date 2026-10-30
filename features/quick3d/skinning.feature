# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-skin.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-model.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-skeleton.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-joint.html
#   https://doc.qt.io/qt-6/quick3d-vertex-skinning.html
#   https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation; each
# Rule is tagged as checked against Qt once qml6 has checked it.

Feature: A mesh bent by the joints of its skin
  A mesh whose vertices carry joint indices and weights is skinned: each
  vertex follows the joints it names, by their weights. A Model's skin
  lists the joints, any nodes, in the order the indices count them, with
  the inverse of each joint's transform when the mesh was bound to it; the
  model's own transform is then ignored. A Skeleton of Joints, each with
  its index, does the same with Model.skeleton and inverseBindPoses; if a
  model has both, the skin is used. A glTF file's skins are drawn the same
  way by a RuntimeLoader, in the pose the file's nodes give.

  # An orthographic camera at magnification 1 shows one scene unit as one
  # pixel, so the view's centre is the scene's origin. The strip is 20
  # units wide and 100 tall, from the origin up. Its lower half follows the
  # first joint, at the origin; its top edge follows the second, 50 units
  # up. Turned a quarter turn about Z, the second joint swings the top of
  # the strip over to the left, around x -50.

  @ljos-only
  Rule: A skin's joints move the vertices that name them

    # The Strip in the module Fixtures is that geometry, unlit by its
    # material.
    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        import Fixtures
        Rectangle {
          width: 200; height: 200; color: "black"
          View3D {
            anchors.fill: parent
            environment: SceneEnvironment { clearColor: "#202020"; backgroundMode: SceneEnvironment.Color }
            OrthographicCamera { z: 600 }
            Node { id: hip }
            Node { id: knee; objectName: "knee"; y: 50 }
            Model {
              objectName: "strip"
              geometry: Strip {}
              skin: Skin {
                objectName: "skin"
                joints: [hip, knee]
                inverseBindPoses: [Qt.matrix4x4(), Qt.matrix4x4(1, 0, 0, 0, 0, 1, 0, -50, 0, 0, 1, 0, 0, 0, 0, 1)]
              }
              materials: DefaultMaterial { lighting: DefaultMaterial.NoLighting; diffuseColor: "red" }
            }
          }
        }
        """

    Scenario: In its bind pose the strip stands straight
      Then the pixel at 100, 25 of the window is within 2 of "#ff0000"
      And the pixel at 100, 75 of the window is within 2 of "#ff0000"

    Scenario: Turning a joint bends what follows it
      When the property "eulerRotation.z" of "knee" is set to 90
      Then the pixel at 100, 75 of the window is within 2 of "#ff0000"
      And the pixel at 100, 15 of the window is "#202020"
      And the pixel at 60, 50 of the window is within 2 of "#ff0000"

    Scenario: A skinned model's own transform is ignored
      When the property "x" of "strip" is set to 60
      Then the pixel at 100, 75 of the window is within 2 of "#ff0000"
      And the pixel at 160, 75 of the window is "#202020"

  @ljos-only
  Rule: A skeleton of joints does the same by their indices

    Scenario: A joint of a skeleton bends the strip
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        import Fixtures
        Rectangle {
          width: 200; height: 200; color: "black"
          View3D {
            anchors.fill: parent
            environment: SceneEnvironment { clearColor: "#202020"; backgroundMode: SceneEnvironment.Color }
            OrthographicCamera { z: 600 }
            Skeleton {
              id: skeleton
              Joint {
                index: 0; skeletonRoot: skeleton
                Joint { index: 1; skeletonRoot: skeleton; y: 50; eulerRotation.z: 90 }
              }
            }
            Model {
              geometry: Strip {}
              skeleton: skeleton
              inverseBindPoses: [Qt.matrix4x4(), Qt.matrix4x4(1, 0, 0, 0, 0, 1, 0, -50, 0, 0, 1, 0, 0, 0, 0, 1)]
              materials: DefaultMaterial { lighting: DefaultMaterial.NoLighting; diffuseColor: "red" }
            }
          }
        }
        """
      Then the pixel at 100, 75 of the window is within 2 of "#ff0000"
      And the pixel at 100, 15 of the window is "#202020"
      And the pixel at 60, 50 of the window is within 2 of "#ff0000"

  Rule: A glTF file's skin is drawn in the pose its nodes give

    # skin.gltf, made by features/quick3d/models/gen.py, is the strip with
    # its two joints as nodes, the second turned a quarter turn about Z.
    Scenario: A loaded skinned mesh is bent by its joints
      Given the model file "skin.gltf"
      And the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Rectangle {
          width: 200; height: 200; color: "black"
          View3D {
            anchors.fill: parent
            environment: SceneEnvironment { clearColor: "#202020"; backgroundMode: SceneEnvironment.Color }
            OrthographicCamera { z: 300 }
            RuntimeLoader { objectName: "loader"; source: "skin.gltf" }
          }
        }
        """
      When 100 milliseconds pass
      Then the property "status" of "loader" is 1
      And the pixel at 100, 75 of the window is within 2 of "#ff0000"
      And the pixel at 100, 15 of the window is "#202020"
      And the pixel at 60, 50 of the window is within 2 of "#ff0000"
      And the log has no warning matching "skin"
