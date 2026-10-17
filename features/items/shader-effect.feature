# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-shadereffect.html
#   https://doc.qt.io/qt-6/qml-qtquick-gridmesh.html
# Verified on Qt 6.11.2 (qml6, offscreen) for the properties. Qt's
# software scene graph, which the Qt backend uses, draws no ShaderEffect
# and leaves every one Uncompiled, valid or not, so status, log and
# pixels are pinned on Ljós only. They were checked against qml6 on GL
# (xcb in the off-screen desktop): a valid shader is Compiled before
# onCompleted, a missing one is Error with the three warnings below, and
# an effect with no shaders draws its source times its opacity.
#
# Qt reads `.qsb` files, which Ljós does not: it reads the GLSL source
# beside one (`X.frag` for `X.frag.qsb`) and compiles it, in the
# background, or a `X.frag.ljsh` that `ljos shaders` made from it
# (docs/ARCHITECTURE.md section 7.1). Ljós's reference renderer, which
# the ledger reads pixels from, draws custom shaders as nothing, as Qt's
# software scene graph does; the GL painter draws them.

Feature: ShaderEffect
  A ShaderEffect draws its rectangle with a vertex and a fragment shader.
  Its other properties are the shaders' uniforms, by name, and an item
  that gives pixels (an Image, a ShaderEffectSource) is a texture.

  @qt-parity
  Rule: Properties

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          width: 100; height: 100
          ShaderEffect { id: fx; objectName: "fx"; width: 40; height: 40 }
          ShaderEffect {
            id: grid; objectName: "grid"; width: 40; height: 40
            mesh: GridMesh { resolution: Qt.size(4, 2) }
          }
          QtObject {
            objectName: "probe"
            property real meshWidth: fx.mesh.width
            property real meshHeight: fx.mesh.height
            property real gridColumns: grid.mesh.resolution.width
            property real gridRows: grid.mesh.resolution.height
          }
        }
        """

    Scenario Outline: Defaults
      Then the property "<property>" of "fx" is <value>

      Examples:
        | property              | value |
        | fragmentShader        | ""    |
        | vertexShader          | ""    |
        | blending              | true  |
        | cullMode              | 0     |
        | supportsAtlasTextures | false |
        | log                   | ""    |

    Scenario: The default mesh is one cell
      Then the property "meshWidth" of "probe" is 1
      And the property "meshHeight" of "probe" is 1

    Scenario: A GridMesh gives the mesh its resolution
      Then the property "gridColumns" of "probe" is 4
      And the property "gridRows" of "probe" is 2

  @qt-parity
  Rule: Enumerations

    Scenario Outline: ShaderEffect.<name> is <value>
      Given the QML:
        """qml
        import QtQuick
        QtObject { objectName: "e"; property int v: ShaderEffect.<name> }
        """
      Then the property "v" of "e" is <value>

      Examples:
        | name             | value |
        | NoCulling        | 0     |
        | BackFaceCulling  | 1     |
        | FrontFaceCulling | 2     |
        | Compiled         | 0     |
        | Uncompiled       | 1     |
        | Error            | 2     |

  @ljos-only
  Rule: Compiling the shaders

    Background:
      Given the text file "red.frag":
        """
        #version 440
        layout(location = 0) in vec2 qt_TexCoord0;
        layout(location = 0) out vec4 fragColor;
        layout(std140, binding = 0) uniform buf {
            mat4 qt_Matrix;
            float qt_Opacity;
            vec4 tint;
        };
        void main() { fragColor = tint * qt_Opacity; }
        """
      And the text file "broken.frag":
        """
        #version 440
        layout(location = 0) out vec4 fragColor;
        void main() { fragColor = nothing; }
        """
      And the text file "only.frag.qsb":
        """
        not a shader Ljós can read
        """

    Scenario: A shader is compiled from the source beside its .qsb
      Given the QML:
        """qml
        import QtQuick
        ShaderEffect {
          objectName: "fx"; width: 40; height: 40
          property color tint: "red"
          fragmentShader: "red.frag.qsb"
        }
        """
      When 50 milliseconds pass
      Then the property "status" of "fx" is 0
      And the property "log" of "fx" is ""
      And the log has no warnings

    Scenario: An effect with no shaders is compiled at once
      Given the QML:
        """qml
        import QtQuick
        ShaderEffect { objectName: "fx"; width: 40; height: 40 }
        """
      Then the property "status" of "fx" is 0

    Scenario: A shader that is not there is an error, as in Qt
      Given the QML:
        """qml
        import QtQuick
        ShaderEffect { objectName: "fx"; width: 40; height: 40; fragmentShader: "gone.frag.qsb" }
        """
      When 50 milliseconds pass
      Then the property "status" of "fx" is 2
      And the log has a warning matching "Failed to find shader .*gone.frag.qsb"
      And the log has a warning matching "ShaderEffect: shader preparation failed for .*gone.frag.qsb"

    Scenario: A .qsb with no source beside it is an error that says why
      Given the QML:
        """qml
        import QtQuick
        ShaderEffect { objectName: "fx"; width: 40; height: 40; fragmentShader: "only.frag.qsb" }
        """
      When 50 milliseconds pass
      Then the property "status" of "fx" is 2
      And the property "log" of "fx" is not ""
      And the log has a warning matching "ShaderEffect: .*only.frag.*GLSL source"

    Scenario: A shader that does not compile is an error with a log
      Given the QML:
        """qml
        import QtQuick
        ShaderEffect { objectName: "fx"; width: 40; height: 40; fragmentShader: "broken.frag" }
        """
      When 50 milliseconds pass
      Then the property "status" of "fx" is 2
      And the property "log" of "fx" is not ""

  @ljos-only
  Rule: Pixels in the reference renderer

    Background:
      Given a 40 by 40 image file "red.png" filled with "#ff0000"
      And the text file "red.frag":
        """
        #version 440
        layout(location = 0) out vec4 fragColor;
        layout(std140, binding = 0) uniform buf { mat4 qt_Matrix; float qt_Opacity; };
        void main() { fragColor = vec4(qt_Opacity, 0.0, 0.0, qt_Opacity); }
        """

    Scenario: An effect with no shaders draws its source times its opacity
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          width: 100; height: 50; color: "#000000"
          Image { id: img; source: "red.png"; visible: false }
          ShaderEffect { width: 40; height: 40; property var source: img }
          ShaderEffect { x: 50; width: 40; height: 40; opacity: 0.5; property var source: img }
        }
        """
      Then the pixel at 20, 20 of the window is "#ff0000"
      And the pixel at 70, 20 of the window is within 2 of "#800000"

    Scenario: A custom shader draws nothing in the reference renderer
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          width: 100; height: 50; color: "#000000"
          ShaderEffect { objectName: "fx"; width: 40; height: 40; fragmentShader: "red.frag.qsb" }
        }
        """
      When 50 milliseconds pass
      Then the property "status" of "fx" is 0
      And the pixel at 20, 20 of the window is "#000000"
