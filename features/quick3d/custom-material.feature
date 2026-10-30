# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-custommaterial.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-textureinput.html
#   https://doc.qt.io/qt-6/qtquick3d-custom.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation; each
# Rule is tagged as checked against Qt once qml6 has checked it.

Feature: CustomMaterial runs the author's shader snippets
  A CustomMaterial draws a model with the vertex and fragment snippets its
  vertexShader and fragmentShader name. A snippet's MAIN is GLSL that reads
  and writes the documented keywords (VERTEX, POSITION, UV0, FRAGCOLOR,
  BASE_COLOR, EMISSIVE_COLOR and the rest). The material's own properties
  are uniforms of the same name: a color is made linear on the way, as the
  built-in materials' colours are, and a TextureInput is a sampler. An
  unshaded material writes FRAGCOLOR to the screen as it is; a shaded one
  is lit, and its light-processor functions replace the built-in lighting
  for the lights they name.

  # A square, 100 units across, faces an orthographic camera, so one scene
  # unit is one pixel and the square covers pixels 50 to 150 in front of a
  # dark grey background.
  Background:
    Given the file "tint.frag":
      """glsl
      void MAIN()
      {
          FRAGCOLOR = vec4(tint.rgb * level, 1.0);
      }
      """
    And the file "shift.vert":
      """glsl
      void MAIN()
      {
          VERTEX.x += offset;
          POSITION = MODELVIEWPROJECTION_MATRIX * vec4(VERTEX, 1.0);
      }
      """
    And the file "picture.frag":
      """glsl
      void MAIN()
      {
          FRAGCOLOR = texture(picture, UV0);
      }
      """
    And the file "glow.frag":
      """glsl
      void MAIN()
      {
          BASE_COLOR = vec4(0.0, 0.0, 0.0, 1.0);
          EMISSIVE_COLOR = vec3(0.5);
      }
      """
    And the file "flat.frag":
      """glsl
      void MAIN()
      {
      }
      void DIRECTIONAL_LIGHT()
      {
          DIFFUSE += LIGHT_COLOR * 0.25;
      }
      void SPECULAR_LIGHT()
      {
      }
      """
    And the file "broken.frag":
      """glsl
      void MAIN() { FRAGCOLOR = vec4(1.0 }
      """
    And a 4 by 4 image file "green.png" filled with "#00ff00"
    And the QML:
      """qml
      import QtQuick; import QtQuick3D
      Rectangle { width: 200; height: 200; color: "black"
        View3D { anchors.fill: parent
          environment: SceneEnvironment { clearColor: "#202020"; backgroundMode: SceneEnvironment.Color }
          OrthographicCamera { z: 600 }
          DirectionalLight { objectName: "sun"; brightness: 0 }
          Model {
            objectName: "unshaded"; source: "#Rectangle"
            materials: CustomMaterial {
              objectName: "custom"; shadingMode: CustomMaterial.Unshaded
              property color tint: "red"; property real level: 1.0; property real offset: 0.0
              vertexShader: "shift.vert"; fragmentShader: "tint.frag"
            }
          }
          Model {
            objectName: "textured"; source: "#Rectangle"; visible: false
            materials: CustomMaterial {
              shadingMode: CustomMaterial.Unshaded; fragmentShader: "picture.frag"
              property TextureInput picture: TextureInput { objectName: "input"; texture: Texture { source: "green.png" } }
            }
          }
          Model { objectName: "emissive"; source: "#Rectangle"; visible: false; materials: CustomMaterial { fragmentShader: "glow.frag" } }
          Model { objectName: "lit"; source: "#Rectangle"; visible: false; materials: CustomMaterial { fragmentShader: "flat.frag" } }
        }
      }
      """

  Rule: A CustomMaterial starts from the values the documentation gives

    Scenario: A new CustomMaterial
      Then the property "shadingMode" of "custom" is 1
      And the property "sourceBlend" of "custom" is 0
      And the property "destinationBlend" of "custom" is 0
      And the property "alwaysDirty" of "custom" is false
      And the property "depthDrawMode" of "custom" is 0
      And the property "enabled" of "input" is true

  Rule: An unshaded material writes FRAGCOLOR as it is

    Scenario: The square shows the colour its fragment snippet writes
      Then the pixel at 100, 100 of the window is within 2 of "#ff0000"
      And the pixel at 20, 20 of the window is within 2 of "#202020"

    Scenario: A real property is a uniform the snippet reads
      When the property "level" of "custom" is set to 0.5
      Then the pixel at 100, 100 of the window is within 2 of "#800000"

    # The colour is made linear for the shader, and an unshaded material's
    # output is not tonemapped back: #808080 is about 0.216, written as #373737.
    Scenario: A color property reaches the shader in linear light
      When the property "tint" of "custom" is set to "#808080"
      Then the pixel at 100, 100 of the window is within 3 of "#373737"

    Scenario: A vertex snippet moves the vertices
      When the property "offset" of "custom" is set to 60
      Then the pixel at 80, 100 of the window is within 2 of "#202020"
      And the pixel at 180, 100 of the window is within 2 of "#ff0000"

  Rule: A TextureInput property is a sampler

    Scenario: The snippet samples the texture
      When the property "visible" of "unshaded" is set to false
      And the property "visible" of "textured" is set to true
      Then the pixel at 100, 100 of the window is within 2 of "#00ff00"

    Scenario: A disabled TextureInput samples opaque black
      When the property "visible" of "unshaded" is set to false
      And the property "visible" of "textured" is set to true
      And the property "enabled" of "input" is set to false
      Then the pixel at 100, 100 of the window is within 2 of "#000000"

  Rule: A shaded material is lit and tonemapped

    # The default tonemapping is linear: 0.5 of light shows as #bcbcbc.
    Scenario: The emissive colour MAIN writes is tonemapped
      When the property "visible" of "unshaded" is set to false
      And the property "visible" of "emissive" is set to true
      Then the pixel at 100, 100 of the window is within 3 of "#bcbcbc"

    # The light's brightness is 1, so its colour is white; the function adds
    # a quarter of it to the diffuse light whatever the angle, and an empty
    # SPECULAR_LIGHT drops the highlight. A quarter shows as #898989.
    Scenario: A DIRECTIONAL_LIGHT function replaces the built-in diffuse light
      When the property "visible" of "unshaded" is set to false
      And the property "visible" of "lit" is set to true
      And the property "brightness" of "sun" is set to 1
      Then the pixel at 100, 100 of the window is within 3 of "#898989"

    Scenario: Without lights the function adds nothing
      When the property "visible" of "unshaded" is set to false
      And the property "visible" of "lit" is set to true
      Then the pixel at 100, 100 of the window is within 2 of "#000000"

  Rule: Blending needs both a source and a destination factor

    Scenario: Additive blending adds the square to what is behind it
      When the property "level" of "custom" is set to 0.5
      And the property "sourceBlend" of "custom" is set to 2
      And the property "destinationBlend" of "custom" is set to 2
      Then the pixel at 100, 100 of the window is within 3 of "#a02020"

    Scenario: One factor alone does not blend
      When the property "level" of "custom" is set to 0.5
      And the property "sourceBlend" of "custom" is set to 2
      Then the pixel at 100, 100 of the window is within 2 of "#800000"

  Rule: A snippet that does not compile draws nothing

    Scenario: A broken snippet is reported and its model is left out
      When the property "fragmentShader" of "custom" is set to "broken.frag"
      Then the log has a warning matching "broken.frag"
      And the pixel at 100, 100 of the window is within 2 of "#202020"
