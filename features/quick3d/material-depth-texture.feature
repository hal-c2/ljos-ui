# Sources:
#   https://doc.qt.io/qt-6/qtquick3d-custom.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation.

Feature: A CustomMaterial reads the depth of the scene
  A material whose snippet names DEPTH_TEXTURE samples the depth of the
  opaque models of the scene, drawn before any of them is coloured: 1.0
  where nothing is, less in front of that. A model that does not write
  depth is not in it, which is how a soft particle sees what is behind it.

  # An orthographic camera at magnification 1 shows one scene unit as one
  # pixel, so the view's centre is the scene's origin. A red square 60
  # units across sits behind, from x 40 to 100 in the window; the blended
  # square that reads the depth is in front, from x 70 to 130.
  Background:
    Given the text file "depth.frag":
      """
      void MAIN()
      {
          vec2 uv = FRAGCOORD.xy / vec2(textureSize(DEPTH_TEXTURE, 0));
          float d = texture(DEPTH_TEXTURE, uv).r;
          FRAGCOLOR = d < 1.0 ? vec4(0.0, 1.0, 0.0, 1.0) : vec4(0.0, 0.0, 1.0, 1.0);
      }
      """

  Scenario Outline: The material sees the opaque model and the empty scene
    Given the QML:
      """qml
      import QtQuick
      import QtQuick3D
      Rectangle {
        width: 200; height: 200; color: "black"
        View3D {
          anchors.fill: parent
          environment: SceneEnvironment {
            clearColor: "#202020"; backgroundMode: SceneEnvironment.Color
            antialiasingMode: <antialiasing>
          }
          OrthographicCamera { z: 600 }
          Model {
            source: "#Rectangle"; x: -30; z: -10; scale: Qt.vector3d(0.6, 0.6, 0.6)
            materials: PrincipledMaterial { lighting: PrincipledMaterial.NoLighting; baseColor: "red" }
          }
          Model {
            source: "#Rectangle"; scale: Qt.vector3d(0.6, 0.6, 0.6)
            materials: CustomMaterial {
              shadingMode: CustomMaterial.Unshaded
              sourceBlend: CustomMaterial.SrcAlpha
              destinationBlend: CustomMaterial.OneMinusSrcAlpha
              fragmentShader: "depth.frag"
            }
          }
        } }
      """
    Then the pixel at 80, 100 of the window is within 2 of "#00ff00"
    And the pixel at 120, 100 of the window is within 2 of "#0000ff"
    And the pixel at 50, 100 of the window is within 2 of "#ff0000"
    And the log has no warnings

    Examples:
      | antialiasing                |
      | SceneEnvironment.NoAA       |
      | SceneEnvironment.MSAA       |
