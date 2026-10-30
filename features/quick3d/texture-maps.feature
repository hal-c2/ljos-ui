# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-principledmaterial.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-texture.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation; each
# Rule is tagged as checked against Qt once qml6 has checked it.

Feature: The maps of a PrincipledMaterial
  A PrincipledMaterial takes its base colour, metalness, roughness, normals,
  emissive colour, occlusion and opacity from maps as well as from values.
  The base colour map multiplies baseColor and the emissive map multiplies
  emissiveFactor. Maps of one value read one channel of their image, which
  the material's channel properties name: metalness blue, roughness green,
  occlusion red and opacity alpha unless they say otherwise. A normal map
  bends the surface's normals, as much as normalStrength says.

  # A square 200 units across fills the view, one scene unit to one pixel,
  # and faces the camera and a sun that shines straight at it. The base
  # colour map is split into four quadrants, red top left, green top right,
  # blue bottom left and white bottom right. The other maps start neutral.
  Background:
    Given a 64 by 64 image file "quadrants.png" of quadrants "#ff0000, #00ff00, #0000ff, #ffffff"
    And a 4 by 4 image file "white.png" filled with "#ffffff"
    And a 4 by 4 image file "clear.png" filled with "#00ffffff"
    And a 4 by 4 image file "tilted.png" filled with "#ff8080"
    And the QML:
      """qml
      import QtQuick
      import QtQuick3D
      Rectangle {
        width: 200; height: 200; color: "black"
        View3D {
          anchors.fill: parent
          OrthographicCamera { z: 600 }
          DirectionalLight { objectName: "sun"; brightness: 0 }
          Model {
            source: "#Rectangle"; scale: Qt.vector3d(2, 2, 1)
            materials: PrincipledMaterial {
              objectName: "pbr"; lighting: PrincipledMaterial.NoLighting
              baseColorMap: Texture { source: "quadrants.png" }
              emissiveMap: Texture { objectName: "glow"; source: "white.png" }
              opacityMap: Texture { objectName: "opacity"; source: "white.png" }
              normalMap: Texture { objectName: "normals" }
            }
          }
        }
      }
      """

  Rule: The channels start where the documentation says

    # Material.R, G, B and A are 0, 1, 2 and 3.
    Scenario: A new PrincipledMaterial's channels and strengths
      Then the property "metalnessChannel" of "pbr" is 2
      And the property "roughnessChannel" of "pbr" is 1
      And the property "occlusionChannel" of "pbr" is 0
      And the property "opacityChannel" of "pbr" is 3
      And the property "normalStrength" of "pbr" is 1
      And the property "occlusionAmount" of "pbr" is 1

  Rule: The base colour map multiplies the base colour

    Scenario: An unlit material shows its base colour map
      Then the pixel at 50, 50 of the window is within 2 of "#ff0000"
      And the pixel at 150, 50 of the window is within 2 of "#00ff00"
      And the pixel at 50, 150 of the window is within 2 of "#0000ff"
      And the pixel at 150, 150 of the window is within 2 of "#ffffff"

    Scenario: A red base colour keeps only the map's red
      When the property "baseColor" of "pbr" is set to "#ff0000"
      Then the pixel at 150, 150 of the window is within 2 of "#ff0000"
      And the pixel at 150, 50 of the window is within 2 of "#000000"

  Rule: The emissive map multiplies the emissive factor

    Scenario: A lit material with no light shows its emissive map
      When the property "lighting" of "pbr" is set to 1
      And the property "emissiveFactor.x" of "pbr" is set to 1
      And the property "emissiveFactor.y" of "pbr" is set to 1
      And the property "emissiveFactor.z" of "pbr" is set to 1
      And the property "source" of "glow" is set to "quadrants.png"
      Then the pixel at 50, 50 of the window is within 2 of "#ff0000"
      And the pixel at 50, 150 of the window is within 2 of "#0000ff"

    Scenario: With an emissive factor of 0 the emissive map gives no light
      When the property "lighting" of "pbr" is set to 1
      Then the pixel at 150, 150 of the window is within 2 of "#000000"

  Rule: The opacity map's channel multiplies the opacity

    Scenario: Where the opacity map is clear the material is not drawn
      When the property "source" of "opacity" is set to "clear.png"
      Then the pixel at 150, 150 of the window is within 2 of "#000000"

    Scenario: A channel that is opaque in the same map draws the material
      When the property "source" of "opacity" is set to "clear.png"
      And the property "opacityChannel" of "pbr" is set to 0
      Then the pixel at 150, 150 of the window is within 2 of "#ffffff"

  # The tilted map's colour #ff8080 is a normal along the surface, at right
  # angles to the sun's light.
  Rule: A normal map bends the light the surface takes

    Scenario: A normal map at right angles to the light leaves the surface dark
      When the property "lighting" of "pbr" is set to 1
      And the property "brightness" of "sun" is set to 1
      And the property "source" of "normals" is set to "tilted.png"
      Then the pixel at 150, 150 of the window is within 12 of "#000000"

    Scenario: A normal strength of 0 leaves the normals as they are
      When the property "lighting" of "pbr" is set to 1
      And the property "brightness" of "sun" is set to 1
      And the property "source" of "normals" is set to "tilted.png"
      And the property "normalStrength" of "pbr" is set to 0
      Then the pixel at 150, 150 of the window is within 40 of "#ffffff"
