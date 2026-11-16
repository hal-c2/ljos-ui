# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-defaultmaterial.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-sceneenvironment.html#lightProbe-prop
#   https://doc.qt.io/qt-6/qml-qtquick3d-sceneenvironment.html#probeOrientation-prop
# The colours are what Qt 6.11 paints of the same scene, as hal-c2/ljos-web
# measured them for its own renderers (tests/quick3d.spec.js, probe3d).

Feature: A DefaultMaterial lit by a light probe
  A DefaultMaterial under a scene's light probe takes its light from all
  round, and gives back the probe as its shine says: the rougher, the
  more blurred, up to the first level of the probe that is wholly rough.
  How much it gives back is told by the way it gives back as the probe is
  turned, so a turned probe changes the shine and not only what it shows.

  # Two balls lit by nothing but a picture of two rows of four colours
  # laid round them: one dull, one with a shine a little rough.
  Background:
    Given a 64 by 32 image file "sky.png" of tiles "#ff0000, #00ff00, #0000ff, #ffff00 / #808080, #ffffff, #00ffff, #ff00ff"
    And the QML:
      """qml
      import QtQuick
      import QtQuick3D
      Rectangle {
        width: 400; height: 300; color: "#202020"
        View3D {
          anchors.fill: parent
          environment: SceneEnvironment {
            objectName: "env"
            backgroundMode: SceneEnvironment.Color; clearColor: "#202020"
            lightProbe: Texture { source: "sky.png" }
          }
          PerspectiveCamera { z: 259.8; clipNear: 1 }
          component Ball: Model { source: "#Sphere"; scale: Qt.vector3d(0.6, 0.6, 0.6) }
          Ball { x: -160; y: -70; materials: DefaultMaterial { } }
          Ball { x: -80; y: -70; materials: DefaultMaterial { specularAmount: 1; specularRoughness: 0.3 } }
        }
      }
      """

  Rule: Its shine is blurred as far as the probe's first wholly rough level

    Scenario: The dull ball and the shining one, the probe as it is
      Then the pixel at 40, 220 of the window is within 4 of "#83a8c2"
      And the pixel at 120, 220 of the window is within 4 of "#97aefa"
      And the pixel at 108, 208 of the window is within 4 of "#eee9ac"
      And the pixel at 132, 208 of the window is within 4 of "#7db9ef"
      And the pixel at 108, 232 of the window is within 4 of "#eba8fb"
      And the pixel at 132, 232 of the window is within 4 of "#88efff"

  Rule: How much it gives back is told by the way it gives back as the probe is turned

    Scenario: The probe tilted
      When the property "probeOrientation" of "env" is bound to "Qt.vector3d(40, 0, 0)"
      Then the pixel at 40, 220 of the window is within 4 of "#8eb0ca"
      And the pixel at 120, 220 of the window is within 4 of "#9decff"

    Scenario: The probe turned a quarter about the way up
      When the property "probeOrientation" of "env" is bound to "Qt.vector3d(0, 90, 0)"
      Then the pixel at 40, 220 of the window is within 4 of "#92c0aa"
      And the pixel at 120, 220 of the window is within 4 of "#8ae8b6"
