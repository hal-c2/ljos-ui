# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-reflectionprobe.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-principledmaterial.html
# The colours are what Qt 6.11 paints of the same scene, as hal-c2/ljos-web
# measured them for its own renderers (tests/quick3d.spec.js, mirrors3d).

Feature: The light a reflection probe gives from all round
  A rough surface inside a probe's box takes its light from all round the
  probe. Qt sums that over few ways, a quarter as many as a side of the
  probe's cube is across, each read of the cube as the probe saw it; the
  surface is as bright as those ways say, and not as an even blur of the
  whole cube would make it.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick3D
      Rectangle {
        width: 100; height: 100; color: "white"
        component Wall: Model {
          property color colour
          source: "#Cube"; scale: Qt.vector3d(2, 2, 2)
          materials: PrincipledMaterial { lighting: PrincipledMaterial.NoLighting; baseColor: colour }
        }
        View3D {
          anchors.fill: parent
          environment: SceneEnvironment { backgroundMode: SceneEnvironment.Color; clearColor: "#204060" }
          OrthographicCamera { z: 500 }
          ReflectionProbe { boxSize: Qt.vector3d(2000, 2000, 2000) }
          Model {
            source: "#Sphere"; scale: Qt.vector3d(0.8, 0.8, 0.8); receivesReflections: true
            materials: PrincipledMaterial { metalness: 0; roughness: 1; baseColor: "white" }
          }
          Wall { x: 300; colour: "#ff0000" }
          Wall { x: -300; colour: "#00ff00" }
          Wall { y: 300; colour: "#0000ff" }
          Wall { y: -300; colour: "#ffff00" }
          Wall { z: 900; scale: Qt.vector3d(4, 4, 4); colour: "#804020" }
        }
      }
      """

  Rule: A surface that is no metal is lit by the probe from all round, as Qt sums it

    Scenario: The ball facing the camera has the brown wall behind the camera most
      Then the pixel at 50, 50 of the window is within 3 of "#534830"

    Scenario: The ball facing a wall is lit most by that wall
      Then the pixel at 78, 50 of the window is within 3 of "#7b2922"
      And the pixel at 22, 50 of the window is within 3 of "#347722"
      And the pixel at 50, 22 of the window is within 3 of "#352c6f"
