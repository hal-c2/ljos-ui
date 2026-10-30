# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-model.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-orthographiccamera.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation; each
# Rule is tagged as checked against Qt once qml6 has checked it.

Feature: The built-in meshes
  A Model's source names a built-in mesh: "#Cube", "#Rectangle", "#Sphere",
  "#Cylinder" or "#Cone". Each is 100 units across: the sphere's diameter,
  the cylinder's height and diameter, the cone's height and the diameter of
  its base. The cylinder stands along Y about its origin; the cone stands on
  its base at the origin and points up Y.

  # An orthographic camera at magnification 1 shows one scene unit as one
  # pixel, so the view's centre is the scene's origin and the meshes'
  # outlines fall at known pixels. The pixels checked keep well inside or
  # outside them, clear of the facets of a round mesh.
  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick3D
      Rectangle {
        width: 200; height: 200; color: "black"
        View3D {
          anchors.fill: parent
          environment: SceneEnvironment {
            clearColor: "#202020"; backgroundMode: SceneEnvironment.Color
          }
          OrthographicCamera { z: 300 }
          Model {
            objectName: "shape"; source: "#Sphere"
            materials: DefaultMaterial {
              lighting: DefaultMaterial.NoLighting; diffuseColor: "red"
            }
          }
        }
      }
      """

  Rule: A sphere is 100 units across

    Scenario: The sphere covers its centre and stays inside its radius
      Then the pixel at 100, 100 of the window is within 2 of "#ff0000"
      And the pixel at 100, 140 of the window is within 2 of "#ff0000"
      And the pixel at 140, 100 of the window is within 2 of "#ff0000"
      And the pixel at 100, 160 of the window is "#202020"
      And the pixel at 140, 140 of the window is "#202020"

  Rule: A cylinder is 100 units tall and across, standing about its origin

    Scenario: The cylinder seen from the side is a square
      When the property "source" of "shape" is set to "#Cylinder"
      Then the pixel at 100, 100 of the window is within 2 of "#ff0000"
      And the pixel at 55, 55 of the window is within 2 of "#ff0000"
      And the pixel at 145, 145 of the window is within 2 of "#ff0000"
      And the pixel at 100, 40 of the window is "#202020"
      And the pixel at 160, 100 of the window is "#202020"

  Rule: A cone stands on its base at the origin and points up

    Scenario: The cone seen from the side is a triangle above the origin
      When the property "source" of "shape" is set to "#Cone"
      Then the pixel at 100, 95 of the window is within 2 of "#ff0000"
      And the pixel at 100, 10 of the window is within 2 of "#ff0000"
      And the pixel at 60, 95 of the window is within 2 of "#ff0000"
      And the pixel at 100, 110 of the window is "#202020"
      And the pixel at 60, 20 of the window is "#202020"

  Rule: A model with a source that is no mesh draws nothing

    Scenario: An unknown built-in mesh leaves the background
      When the property "source" of "shape" is set to "#Teapot"
      Then the pixel at 100, 100 of the window is "#202020"
