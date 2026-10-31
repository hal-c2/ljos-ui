# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-texturedata.html
#   https://doc.qt.io/qt-6/qquick3dtexturedata.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-texture.html#textureData-prop
#   https://doc.qt.io/qt-6/qml-qtquick3d-helpers-proceduraltexturedata.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation.

@ljos-only
Feature: A texture made of data
  A Texture whose textureData is set shows those pixels and ignores its
  source. A ProceduralTextureData is texture data written in QML: width,
  height and format say how to read the bytes of its textureData, an
  ArrayBuffer. A format that cannot be drawn here is reported, and the
  texture shows nothing.

  Background:
    Given a 4 by 4 image file "blue.png" filled with "#0000ff"
    And the QML:
      """qml
      import QtQuick
      import QtQuick3D
      import QtQuick3D.Helpers
      Rectangle {
        id: scene; objectName: "scene"; property string fill: "red"
        width: 200; height: 200; color: "black"
        function bytes(r, g, b) {
          var u = new Uint8Array(16); for (var i = 0; i < 4; ++i) u.set([r, g, b, 255], i * 4); return u.buffer
        }
        View3D {
          anchors.fill: parent; environment: SceneEnvironment { clearColor: "#202020"; backgroundMode: SceneEnvironment.Color }
          OrthographicCamera { z: 600 }
          Model {
            source: "#Rectangle"; scale: Qt.vector3d(2, 2, 1)
            materials: DefaultMaterial { lighting: DefaultMaterial.NoLighting
              diffuseMap: Texture {
                objectName: "tex"; source: "blue.png"
                textureData: ProceduralTextureData {
                  objectName: "data"; width: 2; height: 2
                  textureData: scene.fill === "red" ? scene.bytes(255, 0, 0) : scene.bytes(0, 255, 0)
                } } }
          }
        }
      }
      """

  Scenario: A new ProceduralTextureData
    Then the property "width" of "data" is 2
    And the property "depth" of "data" is 0
    And the property "format" of "data" is 1
    And the property "hasTransparency" of "data" is false

  Scenario: The texture shows the data, not its source
    Then the pixel at 100, 100 of the window is "#ff0000"

  Scenario: New data is drawn
    When the property "fill" of "scene" is set to "green"
    Then the pixel at 100, 100 of the window is "#00ff00"

  Scenario: Without data the texture shows its source again
    When the property "textureData" of "tex" is set to null
    Then the pixel at 100, 100 of the window is "#0000ff"

  Scenario Outline: Floating point and one channel formats are read
    Given the QML:
      """qml
      import QtQuick
      import QtQuick3D
      import QtQuick3D.Helpers
      Rectangle {
        width: 200; height: 200; color: "black"
        View3D {
          anchors.fill: parent
          OrthographicCamera { z: 600 }
          Model {
            source: "#Rectangle"; scale: Qt.vector3d(2, 2, 1)
            materials: DefaultMaterial {
              lighting: DefaultMaterial.NoLighting
              diffuseMap: Texture {
                textureData: ProceduralTextureData {
                  width: 1; height: 1; format: TextureData.<format>
                  textureData: new <array>([<values>]).buffer
                }
              }
            }
          }
        }
      }
      """
    Then the pixel at 100, 100 of the window is "<colour>"

    Examples:
      | format  | array        | values         | colour  |
      | RGBA8   | Uint8Array   | 0, 0, 255, 255 | #0000ff |
      | RGBA32F | Float32Array | 0, 1, 0, 1     | #00ff00 |
      | R8      | Uint8Array   | 255            | #ff0000 |
      | R32F    | Float32Array | 1              | #ff0000 |

  Scenario: A compressed format is reported
    When the property "format" of "data" is set to 9
    Then the log has a warning matching "compressed"
    And the pixel at 100, 100 of the window is "#ffffff"
