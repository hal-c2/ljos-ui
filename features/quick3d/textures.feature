# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-texture.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-defaultmaterial.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation; each
# Rule is tagged as checked against Qt once qml6 has checked it.

Feature: Textures
  A Texture is an image a material samples over a mesh. Its source is a file
  as an Image's is. scaleU, scaleV, positionU and positionV move the image
  across the mesh's UV coordinates; tilingModeHorizontal and
  tilingModeVertical say what lies beyond the image's edges: Repeat (the
  default) tiles it, MirroredRepeat tiles it mirrored, ClampToEdge stretches
  its edge pixels. magFilter and minFilter say how its pixels are read when
  they are larger or smaller on screen than they are in the image. A
  DefaultMaterial's diffuseMap multiplies its diffuseColor.

  # A square 200 units across fills the view, which shows one scene unit as
  # one pixel. Its image is split into four quadrants, red top left, green
  # top right, blue bottom left and white bottom right, so a pixel tells
  # which part of the image is drawn there.
  Background:
    Given a 64 by 64 image file "quadrants.png" of quadrants "#ff0000, #00ff00, #0000ff, #ffffff"
    And a 2 by 2 image file "small.png" of quadrants "#ff0000, #00ff00, #0000ff, #ffffff"
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
            source: "#Rectangle"; scale: Qt.vector3d(2, 2, 1)
            materials: DefaultMaterial {
              objectName: "plain"; lighting: DefaultMaterial.NoLighting
              diffuseMap: Texture { objectName: "tex"; source: "quadrants.png" }
            }
          }
        }
      }
      """

  Rule: A Texture starts from the values the documentation gives

    Scenario: A new Texture
      Then the property "scaleU" of "tex" is 1
      And the property "scaleV" of "tex" is 1
      And the property "positionU" of "tex" is 0
      And the property "positionV" of "tex" is 0
      And the property "rotationUV" of "tex" is 0
      And the property "pivotU" of "tex" is 0
      And the property "pivotV" of "tex" is 0
      And the property "flipV" of "tex" is false
      And the property "generateMipmaps" of "tex" is false

    # The enumerations count from 0 in the order the documentation lists
    # them: ClampToEdge, MirroredRepeat, Repeat and None, Nearest, Linear.
    Scenario: A new Texture repeats and reads its pixels linearly
      Then the property "tilingModeHorizontal" of "tex" is 2
      And the property "tilingModeVertical" of "tex" is 2
      And the property "minFilter" of "tex" is 2
      And the property "magFilter" of "tex" is 2
      And the property "mipFilter" of "tex" is 0

  Rule: The image covers the mesh as its UV coordinates say

    Scenario: The image is upright on a rectangle
      Then the pixel at 50, 50 of the window is within 2 of "#ff0000"
      And the pixel at 150, 50 of the window is within 2 of "#00ff00"
      And the pixel at 50, 150 of the window is within 2 of "#0000ff"
      And the pixel at 150, 150 of the window is within 2 of "#ffffff"

    Scenario: flipV turns the image upside down
      When the property "flipV" of "tex" is set to true
      Then the pixel at 50, 50 of the window is within 2 of "#0000ff"
      And the pixel at 50, 150 of the window is within 2 of "#ff0000"

    Scenario: A new source replaces the image
      When the property "source" of "tex" is set to "small.png"
      Then the pixel at 150, 150 of the window is within 2 of "#ffffff"

  # u is the UV coordinate times scaleU plus positionU. With scaleU 3 the
  # pixel at x 110 reads u 1.65 and the pixel at x 150 reads u 2.25.
  Rule: Scale, position and tiling say which part of the image a pixel shows

    Scenario: positionU moves the image along U
      When the property "positionU" of "tex" is set to 0.5
      Then the pixel at 50, 50 of the window is within 2 of "#00ff00"
      And the pixel at 150, 50 of the window is within 2 of "#ff0000"

    Scenario Outline: What lies beyond the image's right edge
      When the property "scaleU" of "tex" is set to 3
      And the property "tilingModeHorizontal" of "tex" is set to <mode>
      Then the pixel at 110, 50 of the window is within 2 of "<at 110>"
      And the pixel at 150, 50 of the window is within 2 of "<at 150>"

      Examples:
        | mode | tiling         | at 110  | at 150  |
        | 2    | Repeat         | #00ff00 | #ff0000 |
        | 1    | MirroredRepeat | #ff0000 | #ff0000 |
        | 0    | ClampToEdge    | #00ff00 | #00ff00 |

  # A two by two image on a square of 200 pixels: each image pixel covers
  # 100 screen pixels, and x 100 is the line between red and green.
  Rule: The magnification filter says how an enlarged image reads

    # Whether the blend is in linear light or in sRGB is not documented;
    # the tolerance takes either.
    Scenario: A linear filter blends neighbouring image pixels
      When the property "source" of "tex" is set to "small.png"
      Then the pixel at 100, 50 of the window is within 70 of "#9e9e00"

    Scenario: A nearest filter keeps each image pixel square
      When the property "source" of "tex" is set to "small.png"
      And the property "magFilter" of "tex" is set to 1
      Then the pixel at 95, 50 of the window is within 2 of "#ff0000"
      And the pixel at 105, 50 of the window is within 2 of "#00ff00"

  Rule: A diffuse map multiplies the diffuse colour

    Scenario: A red material shows the image's red parts
      When the property "diffuseColor" of "plain" is set to "#ff0000"
      Then the pixel at 150, 150 of the window is within 2 of "#ff0000"
      And the pixel at 150, 50 of the window is within 2 of "#000000"

    Scenario: Without its map the material shows its colour
      When the property "diffuseMap" of "plain" is set to null
      Then the pixel at 50, 50 of the window is within 2 of "#ffffff"

    Scenario: A map whose image cannot be read is left out
      When the property "source" of "tex" is set to "missing.png"
      Then the pixel at 50, 50 of the window is within 2 of "#ffffff"
