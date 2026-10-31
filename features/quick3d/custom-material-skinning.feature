# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-custommaterial.html
#   https://doc.qt.io/qt-6/qtquick3d-custom.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-skin.html
#   https://doc.qt.io/qt-6/quick3d-vertex-skinning.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation; each
# Rule is tagged as checked against Qt once qml6 has checked it.

Feature: A CustomMaterial on a skinned model is bent by its skin
  A model with a skin and a CustomMaterial is bent by its joints as it is
  with any other material: a vertex snippet that writes no POSITION is
  skinned after it runs. A snippet that writes POSITION places the vertex
  itself, and reads the skin through BONE_TRANSFORMS, the joints' matrices
  in the order the skin lists them, BONE_NORMAL_TRANSFORMS, the matrices
  for normals, and JOINTS and WEIGHTS, the vertex's joint indices and
  their weights.

  # An orthographic camera at magnification 1 shows one scene unit as one
  # pixel, so the view's centre is the scene's origin. The Strip in the
  # module Fixtures is 20 units wide and 100 tall, from the origin up. Its
  # lower half follows the first joint, at the origin; its top edge follows
  # the second, 50 units up. Turned a quarter turn about Z, the second
  # joint swings the top of the strip over to the left, around x -50.
  @ljos-only
  Rule: The skin bends what the snippet draws

    Background:
      Given the text file "plain.vert":
        """
        void MAIN()
        {
        }
        """
      And the text file "red.frag":
        """
        void MAIN()
        {
            FRAGCOLOR = vec4(1.0, 0.0, 0.0, 1.0);
        }
        """
      And the text file "bones.vert":
        """
        void MAIN()
        {
            mat4 skin = BONE_TRANSFORMS[JOINTS.x] * WEIGHTS.x
                      + BONE_TRANSFORMS[JOINTS.y] * WEIGHTS.y
                      + BONE_TRANSFORMS[JOINTS.z] * WEIGHTS.z
                      + BONE_TRANSFORMS[JOINTS.w] * WEIGHTS.w;
            POSITION = VIEWPROJECTION_MATRIX * skin * vec4(VERTEX, 1.0);
        }
        """
      And the text file "unbent.vert":
        """
        void MAIN()
        {
            POSITION = VIEWPROJECTION_MATRIX * vec4(VERTEX, 1.0);
        }
        """
      And the text file "normal.vert":
        """
        VARYING vec3 bent;
        void MAIN()
        {
            bent = BONE_NORMAL_TRANSFORMS[JOINTS.x] * vec3(0.0, 1.0, 0.0);
        }
        """
      And the text file "normal.frag":
        """
        VARYING vec3 bent;
        void MAIN()
        {
            FRAGCOLOR = vec4(max(-bent.x, 0.0), max(bent.y, 0.0), 0.0, 1.0);
        }
        """
      And the QML:
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
                joints: [hip, knee]
                inverseBindPoses: [Qt.matrix4x4(), Qt.matrix4x4(1, 0, 0, 0, 0, 1, 0, -50, 0, 0, 1, 0, 0, 0, 0, 1)]
              }
              materials: CustomMaterial {
                objectName: "custom"; shadingMode: CustomMaterial.Unshaded
                vertexShader: "plain.vert"; fragmentShader: "red.frag"
              }
            }
          }
        }
        """

    Scenario: In its bind pose the strip stands straight
      Then the pixel at 100, 25 of the window is within 2 of "#ff0000"
      And the pixel at 100, 75 of the window is within 2 of "#ff0000"

    Scenario: A snippet that writes no POSITION is bent by the skin
      When the property "eulerRotation.z" of "knee" is set to 90
      Then the pixel at 100, 75 of the window is within 2 of "#ff0000"
      And the pixel at 100, 15 of the window is "#202020"
      And the pixel at 60, 50 of the window is within 2 of "#ff0000"

    Scenario: A snippet that writes POSITION bends with BONE_TRANSFORMS, JOINTS and WEIGHTS
      When the property "vertexShader" of "custom" is set to "bones.vert"
      And the property "eulerRotation.z" of "knee" is set to 90
      Then the pixel at 100, 75 of the window is within 2 of "#ff0000"
      And the pixel at 100, 15 of the window is "#202020"
      And the pixel at 60, 50 of the window is within 2 of "#ff0000"

    Scenario: A snippet that writes POSITION without the bones is not bent
      When the property "vertexShader" of "custom" is set to "unbent.vert"
      And the property "eulerRotation.z" of "knee" is set to 90
      Then the pixel at 100, 15 of the window is within 2 of "#ff0000"
      And the pixel at 60, 50 of the window is "#202020"

    # The lower half follows the hip, which is not turned, so its up
    # stays up; the top edge follows the knee, which turns up to the left.
    # Near the top edge, at x -48, the normal is nearly all the knee's.
    Scenario: BONE_NORMAL_TRANSFORMS turns a normal with its joint
      When the property "vertexShader" of "custom" is set to "normal.vert"
      And the property "fragmentShader" of "custom" is set to "normal.frag"
      And the property "eulerRotation.z" of "knee" is set to 90
      Then the pixel at 100, 75 of the window is within 2 of "#00ff00"
      And the pixel at 52, 50 of the window is within 24 of "#ff0000"
