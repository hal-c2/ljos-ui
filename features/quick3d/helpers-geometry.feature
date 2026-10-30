# Sources:
#   https://doc.qt.io/qt-6/qtquick3d-helpers-qmlmodule.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-helpers-spheregeometry.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-helpers-cuboidgeometry.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-helpers-planegeometry.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-helpers-gridgeometry.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation.

@ljos-only
Feature: The shape geometries of QtQuick3D.Helpers
  A Model draws a SphereGeometry, CuboidGeometry, PlaneGeometry and the
  others like any geometry. They are centred on the origin, measured in
  scene units, and made again when a property changes.

  # An orthographic camera at magnification 1 shows one scene unit as one
  # pixel, so the view's centre is the scene's origin.
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
            geometry: SphereGeometry { objectName: "shape"; radius: 50 }
            materials: DefaultMaterial {
              lighting: DefaultMaterial.NoLighting; diffuseColor: "red"
            }
          }
        }
      }
      """

  Rule: A sphere has the radius it is given

    Scenario: The sphere covers its centre and stays inside its radius
      Then the pixel at 100, 100 of the window is "#ff0000"
      And the pixel at 65, 100 of the window is "#ff0000"
      And the pixel at 100, 65 of the window is "#ff0000"
      And the pixel at 100, 30 of the window is "#202020"
      And the pixel at 160, 100 of the window is "#202020"

    Scenario: A sphere is ready once it is made
      Then the property "status" of "shape" is 2

    Scenario: A smaller radius draws a smaller sphere
      When the property "radius" of "shape" is set to 20
      Then the pixel at 65, 100 of the window is "#202020"
      And the pixel at 100, 100 of the window is "#ff0000"

    Scenario: Nothing to draw is an error
      When the property "radius" of "shape" is set to 0
      Then the property "status" of "shape" is 3
      And the pixel at 100, 100 of the window is "#202020"
