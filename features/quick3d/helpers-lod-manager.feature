# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-helpers-lodmanager.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation. The
# documentation does not say where the distance is measured to, nor where
# the fade lies about a distance: here it is the manager's own place, and
# the fade is fadeDistance wide, centred on the distance.

@ljos-only
Feature: Levels of detail picked by the camera's distance
  A LodManager shows one of its child nodes at a time, by how far its
  camera is: the first child closer than the first distance, the second
  up to the second distance, and so on. The last child stays shown however
  far the camera goes. Across a distance the levels cross-fade over
  fadeDistance: both are shown, the nearer level's opacity falling as the
  farther one's rises. Without a camera the children stay as declared.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick3D
      import QtQuick3D.Helpers
      Rectangle {
        width: 100; height: 100; color: "black"
        View3D {
          anchors.fill: parent; environment: SceneEnvironment { clearColor: "#202020"; backgroundMode: SceneEnvironment.Color }
          PerspectiveCamera { id: cam; objectName: "cam"; z: 300 }
          LodManager {
            objectName: "lod"; camera: cam; distances: [400, 800]
            Model { objectName: "near"; source: "#Cube"; materials: DefaultMaterial { diffuseColor: "red"; lighting: DefaultMaterial.NoLighting } }
            Model { objectName: "middle"; source: "#Cube"; materials: DefaultMaterial { diffuseColor: "lime"; lighting: DefaultMaterial.NoLighting } }
            Model { objectName: "far"; source: "#Cube"; materials: DefaultMaterial { diffuseColor: "blue"; lighting: DefaultMaterial.NoLighting } }
          }
          LodManager {
            objectName: "loose"
            distances: [400]
            Node { objectName: "first" }
            Node { objectName: "second" }
          }
        }
      }
      """

  Scenario: A new LodManager
    Then the property "camera" of "loose" is null
    And the property "fadeDistance" of "loose" is 0

  Scenario: A close camera sees the first child
    Then the property "visible" of "near" is true
    And the property "visible" of "middle" is false
    And the property "visible" of "far" is false
    And the pixel at 50, 50 of the window is "#ff0000"

  Scenario: Past the first distance the second child shows
    When the property "z" of "cam" is set to 600
    Then the property "visible" of "near" is false
    And the property "visible" of "middle" is true
    And the pixel at 50, 50 of the window is "#00ff00"

  Scenario: Past the last distance the last child shows
    When the property "z" of "cam" is set to 2000
    Then the property "visible" of "middle" is false
    And the property "visible" of "far" is true
    And the pixel at 50, 50 of the window is "#0000ff"

  Scenario: The distance is measured from the camera's place in the scene
    When the property "x" of "cam" is set to 500
    Then the property "visible" of "near" is false
    And the property "visible" of "middle" is true

  Scenario: Coming back shows the first child again
    When the property "z" of "cam" is set to 2000
    And the property "z" of "cam" is set to 300
    Then the property "visible" of "near" is true
    And the property "visible" of "far" is false

  Scenario: Across a distance the levels cross-fade
    When the property "fadeDistance" of "lod" is set to 100
    And the property "z" of "cam" is set to 425
    Then the property "visible" of "near" is true
    And the property "visible" of "middle" is true
    And the property "opacity" of "near" is within 0.01 of 0.25
    And the property "opacity" of "middle" is within 0.01 of 0.75
    And the property "visible" of "far" is false

  Scenario: Out of the fade a level is fully opaque
    When the property "fadeDistance" of "lod" is set to 100
    And the property "z" of "cam" is set to 600
    Then the property "opacity" of "middle" is 1
    And the property "visible" of "near" is false

  Scenario: Without a camera the children stay as declared
    Then the property "visible" of "first" is true
    And the property "visible" of "second" is true
