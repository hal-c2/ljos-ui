# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-effect.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-pass.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-buffer.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-bufferinput.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-shader.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-setuniformvalue.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-sceneenvironment.html#effects-prop
#   https://doc.qt.io/qt-6/qtquick3d-custom.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation; each
# Rule is tagged as checked against Qt once qml6 has checked it.

Feature: Effects post-process the rendered scene
  The effects of a SceneEnvironment run, in the order of the list, on the
  scene once it is drawn. Each Effect is a list of passes; a pass runs its
  fragment snippet over the whole view, reading the picture so far as INPUT
  at INPUT_UV and writing FRAGCOLOR. A pass whose output is a Buffer writes
  there, and a later pass reads that buffer through a BufferInput. The
  scene reaches the effects before it is tonemapped, and the last pass's
  result is tonemapped. An Effect's own properties are uniforms of the
  same name, and a SetUniformValue changes one for its pass alone.

  # Stage is a view with an orthographic camera and a red square, 100 units
  # across, that covers pixels 50 to 150, with a blue backdrop when asked.
  # Flat squares are unlit, so each shows its colour as it is: red and blue
  # are 1 in linear light.
  Background:
    Given the file "invert.frag":
      """glsl
      void MAIN()
      {
          vec4 c = texture(INPUT, INPUT_UV);
          FRAGCOLOR = vec4(mix(c.rgb, vec3(1.0) - c.rgb, amount), c.a);
      }
      """
    And the file "dim.frag":
      """glsl
      void MAIN()
      {
          FRAGCOLOR = texture(INPUT, INPUT_UV) * vec4(vec3(factor), 1.0);
      }
      """
    And the file "copy.frag":
      """glsl
      void MAIN()
      {
          FRAGCOLOR = texture(INPUT, INPUT_UV);
      }
      """
    And the file "depth.frag":
      """glsl
      void MAIN()
      {
          float d = texture(DEPTH_TEXTURE, INPUT_UV).r;
          FRAGCOLOR = d < 1.0 ? vec4(1.0) : vec4(0.0, 0.0, 0.0, 1.0);
      }
      """
    And the file "Flat.qml":
      """qml
      import QtQuick3D
      Model {
        id: flat
        property color color: "red"
        source: "#Rectangle"
        materials: DefaultMaterial { lighting: DefaultMaterial.NoLighting; diffuseColor: flat.color }
      }
      """
    And the file "Stage.qml":
      """qml
      import QtQuick
      import QtQuick3D
      Rectangle {
        id: stage
        width: 200; height: 200; color: "black"
        property alias environment: view.environment
        property bool backdrop: false
        View3D {
          id: view
          anchors.fill: parent
          OrthographicCamera { z: 600 }
          DirectionalLight { brightness: 0 }
          Flat { z: -50; scale: Qt.vector3d(4, 4, 1); color: "blue"; visible: stage.backdrop }
          Flat {}
        }
      }
      """

  Rule: Effects run in the order of the list over the scene before tonemapping

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Stage {
          environment: SceneEnvironment {
            clearColor: "#202020"; backgroundMode: SceneEnvironment.Color
            effects: [
              Effect {
                objectName: "invert"; property real amount: 0.0
                passes: Pass { shaders: Shader { stage: Shader.Fragment; shader: "invert.frag" } }
              },
              Effect {
                objectName: "dim"; property real factor: 1.0
                Buffer { id: halfway; name: "halfway"; format: Buffer.RGBA16F }
                passes: [
                  Pass { output: halfway; shaders: Shader { shader: "dim.frag" } },
                  Pass { commands: BufferInput { buffer: halfway }; shaders: Shader { shader: "copy.frag" } }
                ]
              }
            ]
          }
          backdrop: true
        }
        """

    Scenario: Effects that change nothing leave the picture as it was
      Then the pixel at 100, 100 of the window is within 2 of "#ff0000"
      And the pixel at 20, 20 of the window is within 2 of "#0000ff"

    Scenario: A pass writes what it computes from INPUT
      When the property "amount" of "invert" is set to 1
      Then the pixel at 100, 100 of the window is within 2 of "#00ffff"
      And the pixel at 20, 20 of the window is within 2 of "#ffff00"

    # Half of red's light is tonemapped to #bc0000.
    Scenario: A pass writes into a buffer the next pass reads
      When the property "factor" of "dim" is set to 0.5
      Then the pixel at 100, 100 of the window is within 3 of "#bc0000"
      And the pixel at 20, 20 of the window is within 3 of "#0000bc"

    # Inverted then halved, red is half cyan; halved then inverted it would
    # be #bcffff.
    Scenario: The second effect reads what the first one wrote
      When the property "amount" of "invert" is set to 1
      And the property "factor" of "dim" is set to 0.5
      Then the pixel at 100, 100 of the window is within 3 of "#00bcbc"

  Rule: A SetUniformValue changes a uniform for its own pass

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Stage {
          environment: SceneEnvironment {
            clearColor: "#202020"; backgroundMode: SceneEnvironment.Color
            effects: Effect {
              objectName: "dim"; property real factor: 1.0
              Buffer { id: once; name: "once" }
              passes: [
                Pass {
                  output: once; shaders: Shader { shader: "dim.frag" }
                  commands: SetUniformValue { target: "factor"; value: 0.5 }
                },
                Pass { commands: BufferInput { buffer: once }; shaders: Shader { shader: "dim.frag" } }
              ]
            }
          }
        }
        """

    # The first pass halves red and the second keeps the property's 1.
    Scenario: Only the pass with the command sees its value
      Then the pixel at 100, 100 of the window is within 3 of "#bc0000"

    # A quarter of red's light is tonemapped to #890000.
    Scenario: The other passes see the property
      When the property "factor" of "dim" is set to 0.5
      Then the pixel at 100, 100 of the window is within 3 of "#890000"

  Rule: A pass can read the scene's depth

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Stage {
          environment: SceneEnvironment {
            clearColor: "#202020"; backgroundMode: SceneEnvironment.Color
            effects: Effect { passes: Pass { shaders: Shader { shader: "depth.frag" } } }
          }
        }
        """

    Scenario: Only the pixels something was drawn on are nearer than the far plane
      Then the pixel at 100, 100 of the window is within 2 of "#ffffff"
      And the pixel at 20, 20 of the window is within 2 of "#000000"

  Rule: A snippet that does not compile leaves the effect out

    Background:
      Given the file "broken.frag":
        """glsl
        void MAIN() { FRAGCOLOR = vec4(1.0 }
        """
      And the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Stage {
          environment: SceneEnvironment {
            clearColor: "#202020"; backgroundMode: SceneEnvironment.Color
            effects: Effect { passes: Pass { shaders: Shader { shader: "broken.frag" } } }
          }
        }
        """

    Scenario: The scene is drawn without the broken effect
      Then the log has a warning matching "broken.frag"
      And the pixel at 100, 100 of the window is within 2 of "#ff0000"
