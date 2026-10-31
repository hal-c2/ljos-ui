# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-node.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-model.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-defaultmaterial.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-material.html
#   https://doc.qt.io/qt-6/qtqml-typesystem-enumerations.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation; each
# Rule is tagged as checked against Qt once qml6 has checked it.

Feature: What a scene's objects say, read the same however they say it
  A View3D draws what its objects' members say. A member says the same
  thing whichever of its forms it is written in: an enumeration by its
  name or by its number, a node's place by `x` or by `position.x`. A
  member left out says what Qt's documentation gives as its default, and
  writing that default out draws nothing different.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick3D
      Rectangle {
        width: 100; height: 100; color: "black"
        View3D {
          objectName: "view"; anchors.fill: parent
          environment: SceneEnvironment {
            objectName: "env"; clearColor: "#202020"
            backgroundMode: SceneEnvironment.Color
          }
          PerspectiveCamera { objectName: "camera"; z: 300 }
          Node { objectName: "pair"; property real at: 0; x: at; z: at * 2 }
          DirectionalLight { objectName: "sun" }
          Model {
            objectName: "cube"; source: "#Cube"
            function placeAt(across) { position = Qt.vector3d(across, 0, 0) }
            materials: DefaultMaterial {
              objectName: "paint"
              lighting: DefaultMaterial.NoLighting; diffuseColor: "red"
            }
          }
        }
      }
      """

  Rule: An enumeration draws the same by its number as by its name

    # NoLighting is 0: unlit, the cube shows its diffuse colour as it is.
    Scenario: NoLighting given by its number
      When the property "lighting" of "paint" is set to 0
      Then the pixel at 50, 50 of the window is within 2 of "#ff0000"

    # NoCulling (2) draws the faces the camera sees from inside; the
    # default, BackFaceCulling (0), leaves the cube's own back faces out,
    # which a camera outside never sees: the two draw alike.
    Scenario Outline: The material's culling given by number
      When the property "cullMode" of "paint" is set to <value>
      Then the pixel at 50, 50 of the window is within 2 of "#ff0000"
      And the pixel at 2, 2 of the window is "#202020"

      Examples:
        | value |
        | 0     |
        | 2     |

  Rule: A node's place is the same written as `x` or as `position`

    # At x 60 the cube's front face spans about 53 to 88 across the
    # window's 100 pixels: the middle is clear, the right of it is red.
    Scenario: x moves the cube
      When the property "x" of "cube" is set to 60
      Then the pixel at 45, 50 of the window is "#202020"
      And the pixel at 75, 50 of the window is within 2 of "#ff0000"

    Scenario: position moves the cube as x does
      When the method "placeAt" of "cube" is called with 60
      Then the pixel at 45, 50 of the window is "#202020"
      And the pixel at 75, 50 of the window is within 2 of "#ff0000"
      And the property "x" of "cube" is 60

    # Writing one part writes position, and position writes its parts:
    # the part whose binding has not run yet must not get its old value
    # back from that.
    Scenario: Bindings on two parts that change together both hold
      When the property "at" of "pair" is set to 5
      Then the property "x" of "pair" is 5
      And the property "z" of "pair" is 10
      And the property "position.x" of "pair" is 5
      And the property "position.z" of "pair" is 10

  Rule: A member left out says Qt's default

    Scenario Outline: Writing <member> as its default changes nothing drawn
      When the property "<member>" of "<object>" is set to <value>
      Then the pixel at 50, 50 of the window is within 2 of "#ff0000"
      And the pixel at 2, 2 of the window is "#202020"
      And the pixel at 20, 50 of the window is "#202020"

      Examples:
        | object | member        | value     |
        | cube   | scale         | "1, 1, 1" |
        | cube   | visible       | true      |
        | cube   | castsShadows  | true      |
        | camera | fieldOfView   | 60        |
        | camera | clipNear      | 10        |
        | camera | clipFar       | 10000     |
        | sun    | brightness    | 1         |
        | env    | tonemapMode   | 1         |
