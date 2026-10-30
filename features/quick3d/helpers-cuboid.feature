# Sources:
#   https://doc.qt.io/qt-6/qtquick3d-helpers-qmlmodule.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-helpers-spheregeometry.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-helpers-cuboidgeometry.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-helpers-planegeometry.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-helpers-gridgeometry.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation.

@ljos-only
Feature: A cuboid geometry
  A CuboidGeometry is a box centred on the origin, as wide, tall and deep
  as its xExtent, yExtent and zExtent.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick3D
      import QtQuick3D.Helpers
      Rectangle {
        width: 200; height: 200; color: "black"
        View3D {
          anchors.fill: parent
          environment: SceneEnvironment {
            clearColor: "#202020"; backgroundMode: SceneEnvironment.Color
          }
          OrthographicCamera { z: 300 }
          Model {
            objectName: "model"
            geometry: CuboidGeometry { objectName: "shape"; xExtent: 80; yExtent: 40; zExtent: 40 }
            materials: DefaultMaterial {
              lighting: DefaultMaterial.NoLighting; diffuseColor: "red"
              cullMode: DefaultMaterial.NoCulling
            }
          }
        }
      }
      """

  Scenario: The box is as wide and as tall as its extents
    Then the pixel at 70, 100 of the window is "#ff0000"
    And the pixel at 130, 100 of the window is "#ff0000"
    And the pixel at 100, 85 of the window is "#ff0000"
    And the pixel at 100, 70 of the window is "#202020"
    And the pixel at 150, 100 of the window is "#202020"

  Scenario: A longer box covers more
    When the property "yExtent" of "shape" is set to 100
    Then the pixel at 100, 60 of the window is "#ff0000"
