# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-geometry.html
#   https://doc.qt.io/qt-6/qquick3dgeometry.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-model.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine, and a geometry is a type an application writes in
# its own language. The expectations come from the documentation.

@ljos-only
Feature: A geometry an application provides
  An application gives QML a Geometry of its own: vertex data, index
  data, attributes, a stride and a primitive type, set from Rust and
  committed by update(). A Model draws it when its geometry property is
  that object. The Fan in the module Fixtures is such a geometry: a flat
  disc of "corners" corners and radius "radius", facing +Z, around its
  origin, with no normals and no indices.

  # An orthographic camera at magnification 1 shows one scene unit as one
  # pixel, so the view's centre is the scene's origin. A hexagon has a
  # corner on the +X and -X axes; a triangle's left side is at -25.
  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick3D
      import Fixtures
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
            geometry: Fan { objectName: "fan"; corners: 6; radius: 50 }
            materials: DefaultMaterial {
              lighting: DefaultMaterial.NoLighting; diffuseColor: "red"
            }
          }
        }
      }
      """

  Rule: A Model draws its geometry

    Scenario: The disc covers its centre and stays inside its radius
      Then the pixel at 100, 100 of the window is "#ff0000"
      And the pixel at 60, 100 of the window is "#ff0000"
      And the pixel at 100, 60 of the window is "#ff0000"
      And the pixel at 100, 30 of the window is "#202020"
      And the pixel at 160, 100 of the window is "#202020"

  Rule: A geometry that is updated is drawn again

    Scenario: A corner fewer takes a side away
      When the property "corners" of "fan" is set to 3
      Then the pixel at 60, 100 of the window is "#202020"
      And the pixel at 110, 100 of the window is "#ff0000"

    Scenario: A smaller radius draws a smaller disc
      When the property "radius" of "fan" is set to 20
      Then the pixel at 60, 100 of the window is "#202020"
      And the pixel at 100, 100 of the window is "#ff0000"

  Rule: Without a geometry a Model with no source draws nothing

    Scenario: The geometry is taken away
      When the property "geometry" of "model" is set to null
      Then the pixel at 100, 100 of the window is "#202020"
