# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-model.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-view3d.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-skin.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation; each
# Rule is tagged as checked against Qt once qml6 has checked it.

Feature: A Model draws the mesh file at its source
  A Model's source names a mesh file, relative to the QML file that wrote
  it. The file holds sub-meshes. "The sub-mesh uses a material from the
  materials list, corresponding to its index", and "if the number of
  materials is less than the sub-meshes, the last material in the list is
  used for subsequent sub-meshes". The documentation says a model with no
  material renders nothing; here, as with a built-in mesh, it draws with the
  default material. The file is read in the background, so a scene draws
  without it until it arrives.

  # two-squares.mesh, made by features/quick3d/models/gen.py: two sub-meshes,
  # each a square 40 units across, 40 units left and 40 units right of the
  # origin. An orthographic camera at magnification 1 shows one scene unit
  # as one pixel, so the view's centre is the scene's origin: in a window 200
  # across the squares' middles are at 60, 100 and 140, 100.
  Background:
    Given the model file "two-squares.mesh"
    And the model file "jointed.mesh"

  Rule: Each sub-mesh draws with its material

    Scenario: Two sub-meshes draw in two colours
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        View3D {
          width: 200; height: 200
          environment: SceneEnvironment { clearColor: "#202020"; backgroundMode: SceneEnvironment.Color }
          OrthographicCamera { z: 300 }
          Model {
            source: "two-squares.mesh"
            materials: [
              DefaultMaterial { lighting: DefaultMaterial.NoLighting; diffuseColor: "#ff0000" },
              DefaultMaterial { lighting: DefaultMaterial.NoLighting; diffuseColor: "#00ff00" }
            ]
          }
        }
        """
      When 100 milliseconds pass
      Then the pixel at 60, 100 of the window is "#ff0000"
      And the pixel at 140, 100 of the window is "#00ff00"
      And the pixel at 100, 100 of the window is "#1e1e1e"

    Scenario: The last material draws the sub-meshes past the end of the list
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        View3D {
          width: 200; height: 200
          environment: SceneEnvironment { clearColor: "#202020"; backgroundMode: SceneEnvironment.Color }
          OrthographicCamera { z: 300 }
          Model {
            source: "two-squares.mesh"
            materials: DefaultMaterial { lighting: DefaultMaterial.NoLighting; diffuseColor: "#0000ff" }
          }
        }
        """
      When 100 milliseconds pass
      Then the pixel at 60, 100 of the window is "#0000ff"
      And the pixel at 140, 100 of the window is "#0000ff"

    Scenario: Without materials the sub-meshes draw with the default one
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        View3D {
          width: 200; height: 200
          environment: SceneEnvironment { clearColor: "#202020"; backgroundMode: SceneEnvironment.Color }
          OrthographicCamera { z: 300 }
          DirectionalLight { }
          Model { source: "two-squares.mesh" }
        }
        """
      When 100 milliseconds pass
      Then the pixel at 60, 100 of the window is not "#1e1e1e"
      And the pixel at 140, 100 of the window is not "#1e1e1e"

  Rule: The model places what the file holds

    Scenario: The model's own place moves the mesh
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        View3D {
          width: 200; height: 200
          environment: SceneEnvironment { clearColor: "#202020"; backgroundMode: SceneEnvironment.Color }
          OrthographicCamera { z: 300 }
          Model {
            y: 60
            source: "two-squares.mesh"
            materials: DefaultMaterial { lighting: DefaultMaterial.NoLighting; diffuseColor: "#ff0000" }
          }
        }
        """
      When 100 milliseconds pass
      Then the pixel at 60, 40 of the window is "#ff0000"
      And the pixel at 60, 100 of the window is "#1e1e1e"

    Scenario: A new source replaces the mesh
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        View3D {
          width: 200; height: 200
          environment: SceneEnvironment { clearColor: "#202020"; backgroundMode: SceneEnvironment.Color }
          OrthographicCamera { z: 300 }
          Model {
            objectName: "model"
            source: "two-squares.mesh"
            materials: DefaultMaterial { lighting: DefaultMaterial.NoLighting; diffuseColor: "#ff0000" }
          }
        }
        """
      When 100 milliseconds pass
      And the property "source" of "model" is set to "jointed.mesh"
      And 100 milliseconds pass
      Then the pixel at 100, 100 of the window is "#ff0000"
      And the pixel at 60, 100 of the window is "#1e1e1e"

    Scenario: A hidden model's mesh does not draw
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        View3D {
          width: 200; height: 200
          environment: SceneEnvironment { clearColor: "#202020"; backgroundMode: SceneEnvironment.Color }
          OrthographicCamera { z: 300 }
          Model {
            visible: false
            source: "two-squares.mesh"
            materials: DefaultMaterial { lighting: DefaultMaterial.NoLighting; diffuseColor: "#ff0000" }
          }
        }
        """
      When 100 milliseconds pass
      Then the pixel at 60, 100 of the window is "#1e1e1e"

    Scenario: A file's joints follow the model's skin
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        View3D {
          width: 200; height: 200
          environment: SceneEnvironment { clearColor: "#202020"; backgroundMode: SceneEnvironment.Color }
          OrthographicCamera { z: 300 }
          Node { id: joint; x: 60 }
          Model {
            source: "jointed.mesh"
            skin: Skin { joints: [joint]; inverseBindPoses: [Qt.matrix4x4()] }
            materials: DefaultMaterial { lighting: DefaultMaterial.NoLighting; diffuseColor: "#ff0000" }
          }
        }
        """
      When 100 milliseconds pass
      Then the pixel at 160, 100 of the window is "#ff0000"
      And the pixel at 100, 100 of the window is "#1e1e1e"

  Rule: A material's picture is the right way up on what a file holds

    # pictured.mesh, from gen.py: a square 100 across with its UVs from the
    # bottom left corner. quadrants.png has red top left, green top right,
    # blue bottom left and white bottom right.
    Scenario: The top of the image is at the top of the mesh
      Given the model file "pictured.mesh"
      And the model file "quadrants.png"
      And the QML:
        """qml
        import QtQuick
        import QtQuick3D
        View3D {
          width: 200; height: 200
          environment: SceneEnvironment { clearColor: "#202020"; backgroundMode: SceneEnvironment.Color }
          OrthographicCamera { z: 300 }
          Model {
            source: "pictured.mesh"
            materials: PrincipledMaterial {
              lighting: PrincipledMaterial.NoLighting
              baseColorMap: Texture { source: "quadrants.png" }
            }
          }
        }
        """
      When 100 milliseconds pass
      Then the pixel at 75, 75 of the window is within 2 of "#ff0000"
      And the pixel at 125, 75 of the window is within 2 of "#00ff00"
      And the pixel at 75, 125 of the window is within 2 of "#0000ff"
      And the pixel at 125, 125 of the window is within 2 of "#ffffff"

  Rule: A file that cannot be drawn says so and the rest of the scene draws

    Scenario: A missing file warns once and the rest draws
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        View3D {
          width: 200; height: 200
          environment: SceneEnvironment { clearColor: "#202020"; backgroundMode: SceneEnvironment.Color }
          OrthographicCamera { z: 300 }
          Model { x: -40; source: "missing.mesh" }
          Model { x: 40; source: "missing.mesh" }
          Model {
            source: "#Rectangle"; scale: Qt.vector3d(0.2, 0.2, 1)
            materials: DefaultMaterial { lighting: DefaultMaterial.NoLighting; diffuseColor: "#ff0000" }
          }
        }
        """
      When 100 milliseconds pass
      Then the log has 1 warning matching "Model: .*missing\.mesh: "
      And the pixel at 100, 100 of the window is "#ff0000"
      And the pixel at 60, 100 of the window is "#1e1e1e"

    Scenario: A file that is no mesh warns what is wrong with it
      Given the text file "broken.mesh":
        """
        this is not a mesh
        """
      And the QML:
        """qml
        import QtQuick
        import QtQuick3D
        View3D {
          width: 200; height: 200
          OrthographicCamera { z: 300 }
          Model { source: "broken.mesh" }
        }
        """
      When 100 milliseconds pass
      Then the log has 1 warning matching "Model: .*broken\.mesh: .*not a mesh file"

  Rule: A pick names the Model, whichever sub-mesh it hit

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Rectangle {
          objectName: "root"; width: 200; height: 200; color: "black"
          property string hit: ""
          function pickAt(x, y) {
            const result = view.pick(x, y)
            hit = result.objectHit ? result.objectHit.objectName : "nothing"
          }
          View3D {
            id: view; anchors.fill: parent
            OrthographicCamera { z: 300 }
            Model { objectName: "squares"; pickable: true; source: "two-squares.mesh" }
          }
        }
        """

    Scenario: A pick on the first sub-mesh hits the Model
      When 100 milliseconds pass
      And the method "pickAt" of "root" is called with 60, 100
      Then the property "hit" of "root" is "squares"

    Scenario: A pick on the second sub-mesh hits the Model
      When 100 milliseconds pass
      And the method "pickAt" of "root" is called with 140, 100
      Then the property "hit" of "root" is "squares"

    Scenario: A pick between the sub-meshes hits nothing
      When 100 milliseconds pass
      And the method "pickAt" of "root" is called with 100, 100
      Then the property "hit" of "root" is "nothing"

  # morph.mesh, made by features/quick3d/models/gen.py: a square 100 units
  # across with one target that moves every corner 100 units right.
  Rule: A file's morph targets move its vertices by their weights

    Scenario: A target's weight moves the square, and setting it moves it again
      Given the model file "morph.mesh"
      And the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Rectangle {
          width: 200; height: 200; color: "black"
          View3D {
            anchors.fill: parent
            environment: SceneEnvironment { clearColor: "#202020"; backgroundMode: SceneEnvironment.Color }
            OrthographicCamera { z: 300 }
            Model {
              source: "morph.mesh"
              morphTargets: [ MorphTarget { objectName: "t"; weight: 0.5; attributes: MorphTarget.Position } ]
              materials: PrincipledMaterial { lighting: PrincipledMaterial.NoLighting; baseColor: "red" }
            }
          }
        }
        """
      When 100 milliseconds pass
      Then the pixel at 190, 100 of the window is within 2 of "#ff0000"
      And the pixel at 60, 100 of the window is "#1e1e1e"
      When the property "weight" of "t" is set to 0
      And 100 milliseconds pass
      Then the pixel at 60, 100 of the window is within 2 of "#ff0000"
      And the pixel at 190, 100 of the window is "#1e1e1e"

    Scenario: A target moves only the attributes it names
      Given the model file "morph.mesh"
      And the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Rectangle {
          width: 200; height: 200; color: "black"
          View3D {
            anchors.fill: parent
            environment: SceneEnvironment { clearColor: "#202020"; backgroundMode: SceneEnvironment.Color }
            OrthographicCamera { z: 300 }
            Model {
              source: "morph.mesh"
              morphTargets: [ MorphTarget { weight: 1; attributes: MorphTarget.Normal } ]
              materials: PrincipledMaterial { lighting: PrincipledMaterial.NoLighting; baseColor: "red" }
            }
          }
        }
        """
      When 100 milliseconds pass
      Then the pixel at 60, 100 of the window is within 2 of "#ff0000"

  # colored.mesh, made by the same: a square 100 units across, all its corners
  # red.
  Rule: A material can use the colours of a file's vertices

    Scenario Outline: A white surface is drawn in its vertices' colour when enabled
      Given the model file "colored.mesh"
      And the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Rectangle {
          width: 200; height: 200; color: "black"
          View3D {
            anchors.fill: parent
            environment: SceneEnvironment { clearColor: "#202020"; backgroundMode: SceneEnvironment.Color }
            OrthographicCamera { z: 300 }
            Model {
              source: "colored.mesh"
              materials: <type> { lighting: <type>.NoLighting; vertexColorsEnabled: <enabled> }
            }
          }
        }
        """
      When 100 milliseconds pass
      Then the pixel at 100, 100 of the window is within 2 of "<seen>"

      Examples:
        | type               | enabled | seen    |
        | PrincipledMaterial | true    | #ff0000 |
        | PrincipledMaterial | false   | #ffffff |
        | DefaultMaterial    | true    | #ff0000 |
