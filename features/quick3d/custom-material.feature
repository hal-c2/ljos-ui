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
    Given the text file "tint.frag":
      """
      void MAIN()
      {
          FRAGCOLOR = vec4(tint.rgb * level, 1.0);
      }
      """
    And the text file "shift.vert":
      """
      void MAIN()
      {
          VERTEX.x += offset;
          POSITION = MODELVIEWPROJECTION_MATRIX * vec4(VERTEX, 1.0);
      }
      """
    And the text file "picture.frag":
      """
      void MAIN()
      {
          FRAGCOLOR = texture(picture, UV0);
      }
      """
    And the text file "glow.frag":
      """
      void MAIN()
      {
          BASE_COLOR = vec4(0.0, 0.0, 0.0, 1.0);
          EMISSIVE_COLOR = vec3(0.5);
      }
      """
    And the text file "flat.frag":
      """
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
    And the text file "broken.frag":
      """
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
          Model { objectName: "emissive"; source: "#Rectangle"; visible: false; materials: CustomMaterial { objectName: "plain"; fragmentShader: "glow.frag" } }
          Model { objectName: "lit"; source: "#Rectangle"; visible: false; materials: CustomMaterial { fragmentShader: "flat.frag" } }
        }
      }
      """

  Rule: A CustomMaterial starts from the values the documentation gives

    Scenario: A new CustomMaterial
      Then the property "shadingMode" of "plain" is 1
      And the property "sourceBlend" of "plain" is 0
      And the property "destinationBlend" of "plain" is 0
      And the property "alwaysDirty" of "plain" is false
      And the property "depthDrawMode" of "plain" is 0
      And the property "enabled" of "input" is true

  Rule: An unshaded material writes FRAGCOLOR as it is

    Scenario: The square shows the colour its fragment snippet writes
      Then the pixel at 100, 100 of the window is within 2 of "#ff0000"
      And the pixel at 20, 20 of the window is within 2 of "#1e1e1e"

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
      Then the pixel at 80, 100 of the window is within 2 of "#1e1e1e"
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

  # A half-strength red, half there, over a white window, in a view that clears to nothing. Alpha
  # takes the colour's factors unless the material gives it its own: the half
  # of the colour's factor then counts twice and more of the window shows.
  Rule: Alpha blends with its own factors where the material gives them

    Scenario Outline: The alpha left in the view decides how much of the window shows
      Given the text file "half.frag":
        """
        void MAIN()
        {
            FRAGCOLOR = vec4(0.5, 0.0, 0.0, 0.5);
        }
        """
      And the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Rectangle {
          width: 200; height: 200; color: "white"
          View3D {
            anchors.fill: parent
            environment: SceneEnvironment { backgroundMode: SceneEnvironment.Transparent }
            OrthographicCamera { z: 600 }
            Model { source: "#Rectangle"; materials: CustomMaterial {
              shadingMode: CustomMaterial.Unshaded
              fragmentShader: "half.frag"
              sourceBlend: CustomMaterial.SrcAlpha
              destinationBlend: CustomMaterial.OneMinusSrcAlpha
              sourceAlphaBlend: <source>
              destinationAlphaBlend: <destination>
            } }
          }
        }
        """
      Then the pixel at 100, 100 of the window is within 3 of "<pixel>"

      Examples:
        | source               | destination          | pixel   |
        | CustomMaterial.NoBlend | CustomMaterial.NoBlend | #ffbfbf |
        | CustomMaterial.One   | CustomMaterial.Zero  | #bf8080 |

  Rule: Max and min are functions where a function may be called, and constants where one must be

    Scenario: Constants, an array's size, a macro and a NaN all come out as GLSL says
      Given the text file "bounds.frag":
        """
        const float TWO = max(1.0, 2.0);
        float ONE = min(1.0, 2.0);
        // Of a NaN: min gives its first, 1.0, and max the larger of that and 0.0.
        #define SAT(x) max(0.0, min(1.0, x))
        void MAIN()
        {
            const float three = max(1.0, 3.0);
            float room[max(2, 3)];
            room[2] = 1.0;
            float none = zero / zero;
            float seen = SAT(none) + max(0.0, none) + (TWO - 2.0) + (three - 3.0) + (ONE - 1.0);
            FRAGCOLOR = seen == 1.0 ? vec4(0.0, room[2], 0.0, 1.0) : vec4(1.0, 0.0, 0.0, 1.0);
        }
        """
      And the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Rectangle {
          width: 200; height: 200; color: "black"
          View3D {
            anchors.fill: parent
            OrthographicCamera { z: 600 }
            Model { source: "#Rectangle"; materials: CustomMaterial {
              shadingMode: CustomMaterial.Unshaded
              fragmentShader: "bounds.frag"
              property real zero: 0
            } }
          }
        }
        """
      Then the pixel at 100, 100 of the window is within 2 of "#00ff00"

  Rule: A snippet that does not compile draws nothing

    Scenario: A broken snippet is reported and its model is left out
      When the property "fragmentShader" of "custom" is set to "broken.frag"
      Then the log has a warning matching "broken.frag"
      And the pixel at 100, 100 of the window is within 2 of "#1e1e1e"

    # Qt makes a material's shaders when it first draws them, so a model
    # that starts hidden says nothing of its snippets until it is shown.
    Scenario: A hidden model's broken snippet is reported once the model is shown
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Rectangle {
          width: 200; height: 200; color: "black"
          View3D {
            anchors.fill: parent
            environment: SceneEnvironment { clearColor: "#202020"; backgroundMode: SceneEnvironment.Color }
            OrthographicCamera { z: 600 }
            Model { objectName: "floor"; source: "#Rectangle"; visible: false
              materials: CustomMaterial { shadingMode: CustomMaterial.Unshaded; fragmentShader: "broken.frag" } }
          }
        }
        """
      Then the pixel at 100, 100 of the window is within 2 of "#1e1e1e"
      And the log has no warning matching "broken.frag"
      When the property "visible" of "floor" is set to true
      Then the pixel at 100, 100 of the window is within 2 of "#1e1e1e"
      And the log has a warning matching "broken.frag"

  # Measured on qml6 6.12.0 in a window: once a light reaches the model, Qt
  # does not compile a function it calls for each light (DIRECTIONAL_LIGHT,
  # POINT_LIGHT, SPOT_LIGHT, SPECULAR_LIGHT) that is written after a
  # `precision lowp float;` or `precision mediump float;`, whether or not a
  # light of that kind is there: "overloaded functions must have the same
  # parameter precision qualifiers". It counts where the function is written,
  # so a highp default after the low one undoes it. MAIN, AMBIENT_LIGHT,
  # IBL_PROBE and POST_PROCESS compile under any, and an unshaded material
  # calls none. The model is then not drawn, though it still casts its shadow.
  Rule: A light function written under a low default precision does not compile where a light reaches the model

    Scenario Outline: What the default precision is where the function is written
      Given the text file "low.frag":
        """
        precision <precision> float;
        void MAIN() { BASE_COLOR = vec4(0.0, 0.0, 0.0, 1.0); EMISSIVE_COLOR = vec3(0.5); SPECULAR_AMOUNT = 0.0; }
        <then>
        void <function>() { }
        """
      And the QML:
        """qml
        import QtQuick; import QtQuick3D
        Rectangle { width: 200; height: 200; color: "black"
          View3D { anchors.fill: parent
            environment: SceneEnvironment { clearColor: "#202020"; backgroundMode: SceneEnvironment.Color }
            OrthographicCamera { z: 600 }
            DirectionalLight { }
            Model { source: "#Rectangle"; materials: CustomMaterial { fragmentShader: "low.frag" } }
          }
        }
        """
      Then the pixel at 100, 100 of the window is within 3 of "<pixel>"

      Examples:
        | precision | then                    | function          | pixel   |
        | lowp      |                         | DIRECTIONAL_LIGHT | #1e1e1e |
        | mediump   |                         | DIRECTIONAL_LIGHT | #1e1e1e |
        | lowp      |                         | POINT_LIGHT       | #1e1e1e |
        | lowp      |                         | SPOT_LIGHT        | #1e1e1e |
        | mediump   |                         | SPECULAR_LIGHT    | #1e1e1e |
        | highp     |                         | DIRECTIONAL_LIGHT | #ffffff |
        | lowp      | precision highp float;  | DIRECTIONAL_LIGHT | #ffffff |
        | lowp      |                         | AMBIENT_LIGHT     | #ffffff |
        | lowp      |                         | IBL_PROBE         | #ffffff |

    # The snippet is compiled as desktop GLSL, where GL_ES is not defined.
    Scenario Outline: A precision statement the preprocessor leaves out does not count
      Given the text file "low.frag":
        """
        <before>
        <condition>
        precision <precision> float;
        #endif
        void MAIN() { BASE_COLOR = vec4(0.0, 0.0, 0.0, 1.0); EMISSIVE_COLOR = vec3(0.5); SPECULAR_AMOUNT = 0.0; }
        void DIRECTIONAL_LIGHT() { }
        """
      And the QML:
        """qml
        import QtQuick; import QtQuick3D
        Rectangle { width: 200; height: 200; color: "black"
          View3D { anchors.fill: parent
            environment: SceneEnvironment { clearColor: "#202020"; backgroundMode: SceneEnvironment.Color }
            OrthographicCamera { z: 600 }
            DirectionalLight { }
            Model { source: "#Rectangle"; materials: CustomMaterial { fragmentShader: "low.frag" } }
          }
        }
        """
      Then the pixel at 100, 100 of the window is within 3 of "<pixel>"

      Examples:
        | before      | condition     | precision | pixel   |
        |             | #ifdef GL_ES  | mediump   | #ffffff |
        |             | #if 0         | lowp      | #ffffff |
        |             | #ifndef GL_ES | mediump   | #1e1e1e |
        |             | #if 1         | lowp      | #1e1e1e |
        | #define LOW | #ifdef LOW    | lowp      | #1e1e1e |

    Scenario: A light reaches only the models in its scope
      Given the text file "low.frag":
        """
        precision lowp float;
        void MAIN() { BASE_COLOR = vec4(0.0, 0.0, 0.0, 1.0); EMISSIVE_COLOR = vec3(0.5); SPECULAR_AMOUNT = 0.0; }
        void DIRECTIONAL_LIGHT() { }
        """
      And the QML:
        """qml
        import QtQuick; import QtQuick3D
        Rectangle { width: 200; height: 200; color: "black"
          View3D { anchors.fill: parent
            environment: SceneEnvironment { clearColor: "#202020"; backgroundMode: SceneEnvironment.Color }
            OrthographicCamera { z: 600 }
            DirectionalLight { scope: lit }
            Node { id: lit
              Model { x: -50; scale: Qt.vector3d(0.8, 0.8, 1); source: "#Rectangle"; materials: CustomMaterial { fragmentShader: "low.frag" } }
            }
            Model { x: 50; scale: Qt.vector3d(0.8, 0.8, 1); source: "#Rectangle"; materials: CustomMaterial { fragmentShader: "low.frag" } }
          }
        }
        """
      Then the pixel at 50, 100 of the window is within 3 of "#1e1e1e"
      And the pixel at 150, 100 of the window is within 3 of "#bbbbbb"

    Scenario: The model it leaves out is reported
      Given the text file "low.frag":
        """
        precision lowp float;
        void MAIN() { }
        void DIRECTIONAL_LIGHT() { }
        """
      And the QML:
        """qml
        import QtQuick; import QtQuick3D
        View3D { width: 200; height: 200
          DirectionalLight { }
          Model { source: "#Rectangle"; materials: CustomMaterial { fragmentShader: "low.frag" } }
        }
        """
      Then the log has a warning matching "low.frag: fragment shader, line [0-9]+: DIRECTIONAL_LIGHT is written under `precision lowp float;`, so the model is not drawn where a light reaches it"

    # Lights come and go, so the warning is given where the program is made,
    # and says when it matters; qml6 prints nothing until a light reaches it.
    Scenario: A model no light reaches is drawn
      Given the text file "low.frag":
        """
        precision lowp float;
        void MAIN() { BASE_COLOR = vec4(0.0, 0.0, 0.0, 1.0); EMISSIVE_COLOR = vec3(0.5); }
        void DIRECTIONAL_LIGHT() { }
        """
      And the QML:
        """qml
        import QtQuick; import QtQuick3D
        Rectangle { width: 200; height: 200; color: "black"
          View3D { anchors.fill: parent
            environment: SceneEnvironment { clearColor: "#202020"; backgroundMode: SceneEnvironment.Color }
            OrthographicCamera { z: 600 }
            Model { source: "#Rectangle"; materials: CustomMaterial { fragmentShader: "low.frag" } }
          }
        }
        """
      Then the pixel at 100, 100 of the window is within 3 of "#bcbcbc"
      And the log has a warning matching "low.frag: .*not drawn where a light reaches it"

    # The square is 100 units in front of a white floor, and the light comes
    # from 20 degrees to the right: its shadow falls 36 units to the left.
    Scenario: The model left out still casts its shadow
      Given the text file "low.frag":
        """
        precision lowp float;
        void MAIN() { }
        void DIRECTIONAL_LIGHT() { }
        """
      And the QML:
        """qml
        import QtQuick; import QtQuick3D
        Rectangle { width: 200; height: 200; color: "black"
          View3D { anchors.fill: parent
            environment: SceneEnvironment { clearColor: "#202020"; backgroundMode: SceneEnvironment.Color }
            OrthographicCamera { z: 600 }
            DirectionalLight { eulerRotation.y: 20; castsShadow: true; shadowFactor: 100; shadowMapQuality: Light.ShadowMapQualityHigh }
            Model { source: "#Rectangle"; scale: Qt.vector3d(3, 3, 1); materials: DefaultMaterial { diffuseColor: "white" } }
            Model { source: "#Rectangle"; z: 100; materials: CustomMaterial { fragmentShader: "low.frag" } }
          }
        }
        """
      Then the pixel at 100, 100 of the window is within 3 of "#000000"
      And the pixel at 135, 100 of the window is within 8 of "#f8f8f8"

  Rule: A shader's text can be written in QML

    Scenario: The fragment text draws the model
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Rectangle {
          width: 200; height: 200; color: "black"
          View3D {
            anchors.fill: parent
            environment: SceneEnvironment { clearColor: "#202020"; backgroundMode: SceneEnvironment.Color }
            OrthographicCamera { z: 600 }
            Model { source: "#Rectangle"; materials: CustomMaterial {
              shadingMode: CustomMaterial.Unshaded
              __fragmentShaderCode: "void MAIN() { FRAGCOLOR = vec4(0.0, 1.0, 0.0, 1.0); }"
            } }
          }
        }
        """
      Then the pixel at 100, 100 of the window is within 6 of "#00ff00"

    # Many models of the same text, as the tiles of a map are, share what
    # it is translated into; each still reads its own properties.
    Scenario: Models of the same text each draw with their own values
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Rectangle {
          width: 200; height: 200; color: "black"
          View3D {
            anchors.fill: parent
            environment: SceneEnvironment { clearColor: "#202020"; backgroundMode: SceneEnvironment.Color }
            OrthographicCamera { z: 600 }
            Model { source: "#Rectangle"; x: -50; materials: CustomMaterial {
              shadingMode: CustomMaterial.Unshaded; property real level: 1.0
              __fragmentShaderCode: "void MAIN() { FRAGCOLOR = vec4(level, 0.0, 0.0, 1.0); }"
            } }
            Model { source: "#Rectangle"; x: 50; materials: CustomMaterial {
              shadingMode: CustomMaterial.Unshaded; property real level: 0.5
              __fragmentShaderCode: "void MAIN() { FRAGCOLOR = vec4(level, 0.0, 0.0, 1.0); }"
            } }
          }
        }
        """
      Then the pixel at 50, 100 of the window is within 2 of "#ff0000"
      And the pixel at 150, 100 of the window is within 2 of "#800000"

    Scenario: The vertex text moves it
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Rectangle {
          width: 200; height: 200; color: "black"
          View3D {
            anchors.fill: parent
            environment: SceneEnvironment { clearColor: "#202020"; backgroundMode: SceneEnvironment.Color }
            OrthographicCamera { z: 600 }
            Model { source: "#Rectangle"; materials: CustomMaterial {
              shadingMode: CustomMaterial.Unshaded
              __vertexShaderCode: "void MAIN() { VERTEX.x += 200.0; POINT_SIZE = 1.0; POSITION = MODELVIEWPROJECTION_MATRIX * vec4(VERTEX, 1.0); }"
              __fragmentShaderCode: "void MAIN() { FRAGCOLOR = vec4(0.0, 1.0, 0.0, 1.0); }"
            } }
          }
        }
        """
      Then the pixel at 100, 100 of the window is within 6 of "#1e1e1e"

    Scenario: The text can read a texture made later, held in a var
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Rectangle {
          width: 200; height: 200; color: "black"
          View3D {
            anchors.fill: parent
            environment: SceneEnvironment { clearColor: "#202020"; backgroundMode: SceneEnvironment.Color }
            OrthographicCamera { z: 600 }
            Model { source: "#Rectangle"; materials: CustomMaterial {
              id: m
              shadingMode: CustomMaterial.Unshaded
              property var picture: null
              Component.onCompleted: picture = Qt.createQmlObject("import QtQuick3D; TextureInput { }", m)
              __fragmentShaderCode: "void MAIN() { FRAGCOLOR = texture2D(picture, UV0) + vec4(qt_sRGBToLinear(vec3(1.0)), 1.0); }"
            } }
          }
        }
        """
      Then the pixel at 100, 100 of the window is within 6 of "#ffffff"

  Rule: A matrix property reaches a shader

    Scenario: A matrix4x4 moves the colour
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Rectangle {
          width: 200; height: 200; color: "black"
          View3D {
            anchors.fill: parent
            environment: SceneEnvironment { clearColor: "#202020"; backgroundMode: SceneEnvironment.Color }
            OrthographicCamera { z: 600 }
            Model { source: "#Rectangle"; materials: CustomMaterial {
              shadingMode: CustomMaterial.Unshaded
              property matrix4x4 place: Qt.matrix4x4(1, 0, 0, 0, 0, 1, 0, 1, 0, 0, 1, 0, 0, 0, 0, 1)
              __fragmentShaderCode: "void MAIN() { FRAGCOLOR = vec4(0.0, (place * vec4(0.0, 0.0, 0.0, 1.0)).y, 0.0, 1.0); }"
            } }
          }
        }
        """
      Then the pixel at 100, 100 of the window is within 6 of "#00ff00"

  Rule: A lit shader's Fresnel and clearcoat keywords are honoured

    Scenario: A black metal with neither reflects nothing
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Rectangle {
          width: 200; height: 200; color: "black"
          View3D {
            anchors.fill: parent
            environment: SceneEnvironment { clearColor: "#202020"; backgroundMode: SceneEnvironment.Color }
            OrthographicCamera { z: 600 }
            DirectionalLight { eulerRotation.x: 0; brightness: 1 }
            Model { source: "#Rectangle"; materials: CustomMaterial {
              __fragmentShaderCode: "void MAIN() { BASE_COLOR = vec4(0.0, 0.0, 0.0, 1.0); METALNESS = 1.0; ROUGHNESS = 0.5; }"
            } }
          }
        }
        """
      Then the pixel at 100, 100 of the window is "#000000"

    Scenario: A black metal under FRESNEL_BIAS gives nothing back head on, as a light is not bent by it
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Rectangle {
          width: 200; height: 200; color: "black"
          View3D {
            anchors.fill: parent
            environment: SceneEnvironment { clearColor: "#202020"; backgroundMode: SceneEnvironment.Color }
            OrthographicCamera { z: 600 }
            DirectionalLight { eulerRotation.x: 0; brightness: 1 }
            Model { source: "#Rectangle"; materials: CustomMaterial {
              __fragmentShaderCode: "void MAIN() { BASE_COLOR = vec4(0.0, 0.0, 0.0, 1.0); METALNESS = 1.0; ROUGHNESS = 0.5; FRESNEL_BIAS = 1.0; }"
            } }
          }
        }
        """
      Then the pixel at 100, 100 of the window is "#000000"

    Scenario: A clearcoat shines on a black metal
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Rectangle {
          width: 200; height: 200; color: "black"
          View3D {
            anchors.fill: parent
            environment: SceneEnvironment { clearColor: "#202020"; backgroundMode: SceneEnvironment.Color }
            OrthographicCamera { z: 600 }
            DirectionalLight { eulerRotation.x: 0; brightness: 1 }
            Model { source: "#Rectangle"; materials: CustomMaterial {
              __fragmentShaderCode: "void MAIN() { BASE_COLOR = vec4(0.0, 0.0, 0.0, 1.0); METALNESS = 1.0; CLEARCOAT_AMOUNT = 1.0; CLEARCOAT_ROUGHNESS = 0.5; CLEARCOAT_FRESNEL_BIAS = 1.0; }"
            } }
          }
        }
        """
      Then the pixel at 100, 100 of the window is not "#000000"

  Rule: Only the textures a shader names count

    Scenario: A material declares more textures than a shader may sample
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Rectangle {
          width: 200; height: 200; color: "black"
          View3D {
            anchors.fill: parent
            environment: SceneEnvironment { clearColor: "black"; backgroundMode: SceneEnvironment.Color }
            OrthographicCamera { z: 600 }
            Model { source: "#Rectangle"; materials: CustomMaterial {
              shadingMode: CustomMaterial.Unshaded
              property TextureInput a; property TextureInput b; property TextureInput c
              property TextureInput d; property TextureInput e; property TextureInput f
              property TextureInput g; property TextureInput h; property TextureInput i
              property TextureInput used: TextureInput { texture: Texture { sourceItem: Rectangle { width: 4; height: 4; color: "#00ff00" } } }
              __fragmentShaderCode: "void MAIN() { FRAGCOLOR = texture(used, UV0); }"
            } }
          }
        }
        """
      When 100 milliseconds pass
      Then the pixel at 100, 100 of the window is "#00ff00"

  Rule: A vertex shader's texture has no level to choose

    Scenario: A vertex shader samples a texture
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Rectangle {
          width: 200; height: 200; color: "black"
          View3D {
            anchors.fill: parent
            environment: SceneEnvironment { clearColor: "black"; backgroundMode: SceneEnvironment.Color }
            OrthographicCamera { z: 600 }
            Model { source: "#Rectangle"; materials: CustomMaterial {
              shadingMode: CustomMaterial.Unshaded
              property TextureInput map: TextureInput { texture: Texture { sourceItem: Rectangle { width: 4; height: 4; color: "#00ff00" } } }
              __vertexShaderCode: "VARYING vec4 seen; void MAIN() { seen = texture(map, UV0); POSITION = MODELVIEWPROJECTION_MATRIX * vec4(VERTEX, 1.0); }"
              __fragmentShaderCode: "VARYING vec4 seen; void MAIN() { FRAGCOLOR = seen; }"
            } }
          }
        }
        """
      When 100 milliseconds pass
      Then the pixel at 100, 100 of the window is "#00ff00"

    Scenario: A matrix passes from the vertex shader to the fragment shader
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Rectangle {
          width: 200; height: 200; color: "black"
          View3D {
            anchors.fill: parent
            environment: SceneEnvironment { clearColor: "black"; backgroundMode: SceneEnvironment.Color }
            OrthographicCamera { z: 600 }
            Model { source: "#Rectangle"; materials: CustomMaterial {
              shadingMode: CustomMaterial.Unshaded
              __vertexShaderCode: "VARYING mat3 seen; void MAIN() { seen = mat3(vec3(1.0, 0.0, 0.0), vec3(0.0, 1.0, 0.0), vec3(0.0, 0.0, 1.0)); POSITION = MODELVIEWPROJECTION_MATRIX * vec4(VERTEX, 1.0); }"
              __fragmentShaderCode: "VARYING mat3 seen; void MAIN() { FRAGCOLOR = vec4(seen[1], 1.0); }"
            } }
          }
        }
        """
      When 100 milliseconds pass
      Then the pixel at 100, 100 of the window is "#00ff00"

  Rule: A material of no opacity draws nothing

    Scenario: Whatever its alpha mode
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Rectangle {
          width: 200; height: 200; color: "black"
          View3D {
            anchors.fill: parent
            environment: SceneEnvironment { clearColor: "#202020"; backgroundMode: SceneEnvironment.Color }
            OrthographicCamera { z: 600 }
            Model { source: "#Cube"; materials: PrincipledMaterial { opacity: 0; alphaMode: PrincipledMaterial.Opaque; lighting: PrincipledMaterial.NoLighting } }
          }
        }
        """
      Then the pixel at 100, 100 of the window is within 6 of "#1e1e1e"

  Rule: What goes between a material's two shaders

    # The matrix is three of the places a material has: ten in all.
    Scenario: A matrix goes from one to the other
      Given the text file "turned.vert":
        """
        VARYING mat3 turn;
        void MAIN() { turn = mat3(vec3(1.0, 0.0, 0.0), vec3(0.0, 1.0, 0.0), vec3(0.0, 0.0, 1.0)); }
        """
      And the text file "turned.frag":
        """
        VARYING mat3 turn;
        void MAIN() { FRAGCOLOR = vec4(turn * vec3(0.0, 1.0, 0.0), 1.0); }
        """
      And the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Rectangle {
          width: 200; height: 200; color: "black"
          View3D {
            anchors.fill: parent
            OrthographicCamera { z: 600 }
            Model { source: "#Cube"; materials: CustomMaterial { shadingMode: CustomMaterial.Unshaded; vertexShader: "turned.vert"; fragmentShader: "turned.frag" } }
          }
        }
        """
      Then the pixel at 100, 100 of the window is "#00ff00"

    Scenario: As many as ten, with a side to it
      Given the text file "many.vert":
        """
        VARYING vec4 a; VARYING vec4 b; VARYING vec4 c; VARYING vec4 d; VARYING vec4 e;
        VARYING vec4 f; VARYING vec4 g; VARYING vec4 h; VARYING vec4 i; VARYING vec4 j;
        void MAIN() { j = vec4(0.0, 1.0, 0.0, 1.0); }
        """
      And the text file "many.frag":
        """
        VARYING vec4 a; VARYING vec4 b; VARYING vec4 c; VARYING vec4 d; VARYING vec4 e;
        VARYING vec4 f; VARYING vec4 g; VARYING vec4 h; VARYING vec4 i; VARYING vec4 j;
        void MAIN() { FRAGCOLOR = j; }
        """
      And the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Rectangle {
          width: 200; height: 200; color: "black"
          View3D {
            anchors.fill: parent
            OrthographicCamera { z: 600 }
            Model { source: "#Cube"; materials: CustomMaterial { shadingMode: CustomMaterial.Unshaded; vertexShader: "many.vert"; fragmentShader: "many.frag" } }
          }
        }
        """
      Then the pixel at 100, 100 of the window is "#00ff00"

  Rule: A shader's places count up from the bottom, as Qt has them
    UV0 and FRAGCOORD have their y growing upwards, however a picture is held.

    Scenario Outline: The upper half of a rectangle is the half of the high y
      Given the text file "up.frag":
        """
        void MAIN() { FRAGCOLOR = <test> ? vec4(1.0, 0.0, 0.0, 1.0) : vec4(0.0, 0.0, 1.0, 1.0); }
        """
      And the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Rectangle {
          width: 200; height: 200; color: "black"
          View3D {
            anchors.fill: parent
            OrthographicCamera { z: 600 }
            Model { source: "#Rectangle"; scale: Qt.vector3d(2, 2, 1); materials: CustomMaterial { shadingMode: CustomMaterial.Unshaded; fragmentShader: "up.frag" } }
          }
        }
        """
      Then the pixel at 100, 50 of the window is "#ff0000"
      And the pixel at 100, 150 of the window is "#0000ff"

      Examples:
        | test             |
        | UV0.y > 0.5      |
        | FRAGCOORD.y > 100.0 |
        | gl_FragCoord.y > 100.0 |

    Scenario: A picture is still the right way up through UV0
      Given the text file "picture.frag":
        """
        void MAIN() { FRAGCOLOR = texture(picture, UV0); }
        """
      And the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Rectangle {
          width: 200; height: 200; color: "black"
          Rectangle {
            id: drawn
            width: 64; height: 64; layer.enabled: true; visible: false
            Rectangle { width: 64; height: 32; color: "#ff0000" }
            Rectangle { y: 32; width: 64; height: 32; color: "#0000ff" }
          }
          View3D {
            anchors.fill: parent
            OrthographicCamera { z: 600 }
            Model {
              source: "#Rectangle"; scale: Qt.vector3d(2, 2, 1)
              materials: CustomMaterial {
                shadingMode: CustomMaterial.Unshaded
                fragmentShader: "picture.frag"
                property TextureInput picture: TextureInput { texture: Texture { sourceItem: drawn } }
              }
            }
          }
        }
        """
      Then the pixel at 100, 50 of the window is "#ff0000"
      And the pixel at 100, 150 of the window is "#0000ff"

    Scenario: A model's scale does not lengthen the normal a shader reads
      Given the text file "normal.frag":
        """
        void MAIN() {
            FRAGCOLOR = length(VAR_WORLD_NORMAL) > 1.5 ? vec4(1.0, 0.0, 0.0, 1.0) : vec4(0.0, 1.0, 0.0, 1.0);
        }
        """
      And the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Rectangle {
          width: 200; height: 200; color: "black"
          View3D {
            anchors.fill: parent
            OrthographicCamera { z: 600 }
            Model {
              source: "#Rectangle"; scale: Qt.vector3d(0.5, 0.5, 0.5)
              materials: CustomMaterial {
                shadingMode: CustomMaterial.Unshaded
                cullMode: Material.NoCulling
                fragmentShader: "normal.frag"
              }
            }
          }
        }
        """
      Then the pixel at 100, 100 of the window is "#00ff00"

    Scenario: The larger of nothing and what is not a number is nothing, as GLSL defines it
      Given the text file "guarded.frag":
        """
        void MAIN() {
            float guarded = max(0.0, zero / zero);
            FRAGCOLOR = guarded == 0.0 ? vec4(0.0, 1.0, 0.0, 1.0) : vec4(1.0, 0.0, 0.0, 1.0);
        }
        """
      And the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Rectangle {
          width: 200; height: 200; color: "black"
          View3D {
            anchors.fill: parent
            OrthographicCamera { z: 600 }
            Model {
              source: "#Rectangle"
              materials: CustomMaterial {
                shadingMode: CustomMaterial.Unshaded
                cullMode: Material.NoCulling
                fragmentShader: "guarded.frag"
                property real zero: 0
              }
            }
          }
        }
        """
      Then the pixel at 100, 100 of the window is "#00ff00"

  # Checkers of single pixels, 192 across on a square of 64 pixels: three
  # image pixels to a screen pixel, so a pixel read without mipmaps lands on
  # the middle of one square, and one read from a mipmap is the average of
  # the squares beneath it. A texture a custom material reads holds numbers,
  # so the average is of the numbers as they are: half way, #808080.
  # qml6 6.12 drew these same pixels on a GPU in the desktop sandbox; the
  # Qt oracle here runs Qt Quick's software backend, which draws no 3D.
  @ljos-only
  Rule: A mipmapped texture drawn smaller reads the average of what it covers

    Background:
      Given a 192 by 192 image file "checkers.png" of checkers "#ffffff, #000000"
      And the text file "uv.vert":
        """
        VARYING vec2 uv;
        void MAIN()
        {
            uv = UV0;
            POSITION = MODELVIEWPROJECTION_MATRIX * vec4(VERTEX, 1.0);
        }
        """
      And the text file "uv.frag":
        """
        VARYING vec2 uv;
        void MAIN()
        {
            FRAGCOLOR = texture(picture, uv);
        }
        """
      And the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Rectangle {
          width: 200; height: 200; color: "black"
          View3D {
            anchors.fill: parent
            environment: SceneEnvironment { clearColor: "#202020"; backgroundMode: SceneEnvironment.Color }
            OrthographicCamera { z: 600 }
            Model {
              source: "#Rectangle"; scale: Qt.vector3d(0.64, 0.64, 1)
              materials: CustomMaterial {
                shadingMode: CustomMaterial.Unshaded
                vertexShader: "uv.vert"; fragmentShader: "uv.frag"
                property TextureInput picture: TextureInput {
                  texture: Texture { objectName: "tex"; source: "checkers.png"; mipFilter: Texture.Linear }
                }
              }
            }
          }
        }
        """

    Scenario: Without mipmaps each pixel shows one square
      Then the pixel at 100, 100 of the window is within 2 of "#ffffff"
      And the pixel at 101, 100 of the window is within 2 of "#000000"

    Scenario: With mipmaps each pixel shows the average of its squares
      When the property "generateMipmaps" of "tex" is set to true
      Then the pixel at 100, 100 of the window is within 2 of "#808080"
      And the pixel at 101, 100 of the window is within 2 of "#808080"
      And the pixel at 70, 130 of the window is within 2 of "#808080"
