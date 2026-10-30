# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-texture.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-geometry.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-runtimeloader.html
#   https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation; each
# Rule is tagged as checked against Qt once qml6 has checked it.

Feature: A texture read through the second UV set
  A mesh may carry two sets of UV coordinates. A Texture's indexUV says
  which one it is laid out by: 0, the first, or 1, the second; QtQuick3D
  supports two sets, so other values are brought into that range. A glTF
  texture names its set by texCoord, and a RuntimeLoader draws it so.

  # An orthographic camera at magnification 1 shows one scene unit as one
  # pixel, so the view's centre is the scene's origin. The picture has four
  # quadrants: red top left, green top right, blue bottom left, white bottom
  # right. Each square is 100 units across. Its first UV set puts every
  # corner in the red quadrant; its second spreads the whole picture over
  # it, the picture's top at the square's top.

  @ljos-only
  Rule: A Texture's indexUV picks the set its material reads

    # The TwoUvSquare in the module Fixtures is a geometry with both sets.
    Background:
      Given a 64 by 64 image file "quadrants.png" of quadrants "#ff0000, #00ff00, #0000ff, #ffffff"
      And the QML:
        """qml
        import QtQuick
        import QtQuick3D
        import Fixtures
        Rectangle {
          width: 200; height: 200; color: "black"
          View3D {
            anchors.fill: parent
            OrthographicCamera { z: 600 }
            Model {
              geometry: TwoUvSquare {}
              materials: DefaultMaterial {
                lighting: DefaultMaterial.NoLighting
                diffuseMap: Texture { objectName: "map"; source: "quadrants.png" }
              }
            }
          }
        }
        """

    Scenario: The first set is read by default
      Then the pixel at 75, 75 of the window is within 2 of "#ff0000"
      And the pixel at 125, 125 of the window is within 2 of "#ff0000"

    Scenario: indexUV 1 reads the second set
      When the property "indexUV" of "map" is set to 1
      Then the pixel at 75, 75 of the window is within 2 of "#ff0000"
      And the pixel at 125, 75 of the window is within 2 of "#00ff00"
      And the pixel at 75, 125 of the window is within 2 of "#0000ff"
      And the pixel at 125, 125 of the window is within 2 of "#ffffff"

    Scenario: A larger indexUV is brought down to the second set
      When the property "indexUV" of "map" is set to 5
      Then the pixel at 125, 125 of the window is within 2 of "#ffffff"

  Rule: A glTF texture on texCoord 1 is drawn

    # uv2.gltf, made by features/quick3d/models/gen.py, is that square,
    # unlit, its base colour texture the four quadrants read nearest
    # through TEXCOORD_1.
    Scenario: A loaded model's texture follows its second UV set
      Given the model file "uv2.gltf"
      And the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Rectangle {
          width: 200; height: 200; color: "black"
          View3D {
            anchors.fill: parent
            OrthographicCamera { z: 300 }
            RuntimeLoader { objectName: "loader"; source: "uv2.gltf" }
          }
        }
        """
      When 100 milliseconds pass
      Then the property "status" of "loader" is 1
      And the pixel at 75, 75 of the window is within 2 of "#ff0000"
      And the pixel at 125, 75 of the window is within 2 of "#00ff00"
      And the pixel at 75, 125 of the window is within 2 of "#0000ff"
      And the pixel at 125, 125 of the window is within 2 of "#ffffff"
      And the log has no warning matching "TEXCOORD_1"
