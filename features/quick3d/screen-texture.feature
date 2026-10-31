# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-custommaterial.html
#   https://doc.qt.io/qt-6/qtquick3d-custom.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation; each
# Rule is tagged as checked against Qt once qml6 has checked it.

Feature: A CustomMaterial reads the scene behind it
  A material whose fragment snippet names SCREEN_TEXTURE samples the
  colour the view has drawn so far: the background and every opaque model
  whose material does not read the screen itself. Transparent models, and
  models that read the screen, are drawn after it is taken, so they are
  not in it. The texture is the size of the view, so FRAGCOORD over its
  size is the pixel the fragment covers. SCREEN_MIP_TEXTURE is the same
  picture with its mip levels made.

  # An orthographic camera at magnification 1 shows one scene unit as one
  # pixel, so the view's centre is the scene's origin. A red square 60
  # units across sits behind, from x 40 to 100 in the window; the square
  # that reads the screen is in front, from x 70 to 130. Both are unlit.
  # The snippet inverts what it sees, so red shows as cyan and the
  # background, #202020, as #dfdfdf.
  Background:
    Given the text file "invert.frag":
      """
      void MAIN()
      {
          vec2 uv = FRAGCOORD.xy / vec2(textureSize(SCREEN_TEXTURE, 0));
          FRAGCOLOR = vec4(1.0 - texture(SCREEN_TEXTURE, uv).rgb, 1.0);
      }
      """
    And the text file "mip.frag":
      """
      void MAIN()
      {
          vec2 uv = FRAGCOORD.xy / vec2(textureSize(SCREEN_MIP_TEXTURE, 0));
          FRAGCOLOR = vec4(textureLod(SCREEN_MIP_TEXTURE, uv, 0.0).rgb, 1.0);
      }
      """

  Rule: The screen texture holds the opaque scene drawn before the material

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Rectangle {
          width: 200; height: 200; color: "black"
          View3D {
            anchors.fill: parent
            environment: SceneEnvironment { objectName: "env"; clearColor: "#202020"; backgroundMode: SceneEnvironment.Color }
            OrthographicCamera { z: 600 }
            Model {
              source: "#Rectangle"; x: -30; z: -10; scale: Qt.vector3d(0.6, 0.6, 0.6)
              materials: PrincipledMaterial { objectName: "red"; lighting: PrincipledMaterial.NoLighting; baseColor: "red" }
            }
            Model {
              source: "#Rectangle"; scale: Qt.vector3d(0.6, 0.6, 0.6)
              materials: CustomMaterial { objectName: "custom"; shadingMode: CustomMaterial.Unshaded; fragmentShader: "invert.frag" }
            }
            Model {
              objectName: "second"; source: "#Rectangle"; visible: false
              x: 40; z: 10; scale: Qt.vector3d(0.6, 0.6, 0.6)
              materials: CustomMaterial { shadingMode: CustomMaterial.Unshaded; fragmentShader: "invert.frag" }
            }
          }
        }
        """

    Scenario: The material shows the model and the background behind it
      Then the pixel at 80, 100 of the window is within 2 of "#00ffff"
      And the pixel at 120, 100 of the window is within 2 of "#dfdfdf"
      And the pixel at 50, 100 of the window is within 2 of "#ff0000"
      And the pixel at 20, 20 of the window is "#202020"

    Scenario: A transparent model is drawn after the screen is taken
      When the property "opacity" of "red" is set to 0.5
      Then the pixel at 80, 100 of the window is within 2 of "#dfdfdf"

    Scenario: A material that reads the screen does not see another that does
      When the property "visible" of "second" is set to true
      Then the pixel at 120, 100 of the window is within 2 of "#dfdfdf"
      And the pixel at 150, 100 of the window is within 2 of "#dfdfdf"

    Scenario: The screen is taken the same way when the view is multisampled
      When the property "antialiasingMode" of "env" is set to 2
      Then the pixel at 80, 100 of the window is within 2 of "#00ffff"
      And the pixel at 120, 100 of the window is within 2 of "#dfdfdf"

    Scenario: SCREEN_MIP_TEXTURE at its first level is the screen
      When the property "fragmentShader" of "custom" is set to "mip.frag"
      Then the pixel at 80, 100 of the window is within 2 of "#ff0000"
      And the pixel at 120, 100 of the window is within 2 of "#202020"

  Rule: The screen texture holds what the view shows when it is post-processed

    Scenario: Under an ExtendedSceneEnvironment the material inverts what it covers
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        import QtQuick3D.Helpers
        Rectangle {
          width: 200; height: 200; color: "black"
          View3D {
            anchors.fill: parent
            environment: ExtendedSceneEnvironment {
              clearColor: "#202020"; backgroundMode: SceneEnvironment.Color
            }
            OrthographicCamera { z: 600 }
            Model {
              source: "#Rectangle"; x: -30; z: -10; scale: Qt.vector3d(0.6, 0.6, 0.6)
              materials: DefaultMaterial { lighting: DefaultMaterial.NoLighting; diffuseColor: "red" }
            }
            Model {
              source: "#Rectangle"; scale: Qt.vector3d(0.6, 0.6, 0.6)
              materials: CustomMaterial { shadingMode: CustomMaterial.Unshaded; fragmentShader: "invert.frag" }
            }
          }
        }
        """
      Then the pixel at 80, 100 of the window is within 3 of "#00ffff"
      And the pixel at 120, 100 of the window is within 3 of "#dfdfdf"
      And the pixel at 50, 100 of the window is within 3 of "#ff0000"
