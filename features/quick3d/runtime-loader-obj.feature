# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-assetutils-runtimeloader.html
#   https://paulbourke.net/dataformats/obj/
#   https://paulbourke.net/dataformats/mtl/
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation. The
# documentation says RuntimeLoader supports .obj without saying how a file's
# groups or materials are taken: here each group is a model named after it,
# and a material's Kd is linear light, as glTF's factors are.

Feature: A Wavefront OBJ model loaded while the application runs
  A RuntimeLoader reads .obj files as it reads glTF files, and tells them
  apart by the file's extension. Each object or group in the file is a model
  named after it. Its materials come from the .mtl files it names, and their
  pictures from beside those files.

  The model files are made by features/quick3d/models/gen.py:
  - square.obj with square.mtl: one red four-cornered polygon 100 units
    across facing +Z, no normals, in one constant colour;
  - groups.obj with groups.mtl: "left", green, a square 40 units across 50
    units left, and "right", blue, 50 units right, indexed from the end of
    the list;
  - picture.obj with picture.mtl and quadrants.png: a square 100 units
    across whose picture has red top left, green top right, blue bottom
    left and white bottom right.

  # An orthographic camera at magnification 1 shows one scene unit as one
  # pixel, so the view's centre is the scene's origin.
  Background:
    Given the model file "square.obj"
    And the model file "square.mtl"
    And the model file "groups.obj"
    And the model file "groups.mtl"
    And the model file "picture.obj"
    And the model file "picture.mtl"
    And the model file "quadrants.png"
    And the file "Shown.qml":
      """qml
      import QtQuick
      import QtQuick3D
      Rectangle {
        property alias source: loader.source
        property alias loader: loader
        property alias view: view
        width: 200; height: 200; color: "black"
        View3D {
          id: view; anchors.fill: parent
          environment: SceneEnvironment {
            clearColor: "#202020"; backgroundMode: SceneEnvironment.Color
          }
          OrthographicCamera { z: 300 }
          RuntimeLoader { id: loader; objectName: "loader" }
        }
      }
      """

  Rule: The file's faces are drawn with its materials

    Scenario: A polygon with more than three corners is filled
      Given the QML:
        """qml
        Shown { source: "square.obj" }
        """
      When 100 milliseconds pass
      Then the property "status" of "loader" is 1
      And the property "errorString" of "loader" is ""
      And the pixel at 100, 100 of the window is within 2 of "#ff0000"
      And the pixel at 60, 60 of the window is within 2 of "#ff0000"
      And the pixel at 140, 140 of the window is within 2 of "#ff0000"
      And the pixel at 40, 100 of the window is "#202020"

    Scenario: Corners counted from the end of the list and two materials
      Given the QML:
        """qml
        Shown { source: "groups.obj" }
        """
      When 100 milliseconds pass
      Then the property "status" of "loader" is 1
      And the pixel at 50, 100 of the window is within 2 of "#00ff00"
      And the pixel at 150, 100 of the window is within 2 of "#0000ff"
      And the pixel at 100, 100 of the window is "#202020"

    Scenario: A material's picture is drawn the right way up
      Given the QML:
        """qml
        Shown { source: "picture.obj" }
        """
      When 100 milliseconds pass
      Then the property "status" of "loader" is 1
      And the pixel at 75, 75 of the window is within 2 of "#ff0000"
      And the pixel at 125, 75 of the window is within 2 of "#00ff00"
      And the pixel at 75, 125 of the window is within 2 of "#0000ff"
      And the pixel at 125, 125 of the window is within 2 of "#ffffff"

    Scenario: A material's d lets what is behind it show
      Given the text file "glass.obj":
        """
        mtllib glass.mtl
        v -50 -50 0
        v 50 -50 0
        v 50 50 0
        v -50 50 0
        usemtl glass
        f 1 2 3 4
        """
      And the text file "glass.mtl":
        """
        newmtl glass
        illum 0
        Kd 1 1 1
        d 0.5
        """
      And the QML:
        """qml
        Shown { source: "glass.obj" }
        """
      When 100 milliseconds pass
      Then the property "status" of "loader" is 1
      And the pixel at 100, 100 of the window is within 40 of "#a6a6a6"

    Scenario: A file with no normals is lit across its faces
      Given the text file "lit.obj":
        """
        v -50 -50 0
        v 50 -50 0
        v 50 50 0
        v -50 50 0
        f 1 2 3 4
        """
      And the QML:
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
            DirectionalLight {}
            RuntimeLoader { objectName: "loader"; source: "lit.obj" }
          }
        }
        """
      When 100 milliseconds pass
      Then the property "status" of "loader" is 1
      And the pixel at 100, 100 of the window is within 20 of "#fcfcfc"

  Rule: What a file's material lacks leaves the default

    Scenario Outline: <what> is a warning, and the model is drawn
      Given the text file "lost.obj":
        """
        mtllib gone.mtl
        v -50 -50 0
        v 50 -50 0
        v 0 50 0
        usemtl lost
        f 1 2 3
        """
      And the text file "unpictured.obj":
        """
        mtllib unpictured.mtl
        v -50 -50 0
        v 50 -50 0
        v 0 50 0
        vt 0 0
        usemtl unpictured
        f 1/1 2/1 3/1
        """
      And the text file "unpictured.mtl":
        """
        newmtl unpictured
        illum 0
        Kd 0 1 0
        map_Kd gone.png
        """
      And the QML:
        """qml
        Shown { source: "<file>" }
        """
      When 100 milliseconds pass
      Then the property "status" of "loader" is 1
      And the log has a warning matching "<named>"
      And the pixel at 100, 110 of the window is not "#202020"

      Examples:
        | what                   | file           | named    |
        | A .mtl that is missing | lost.obj       | gone.mtl |
        | A picture that is lost | unpictured.obj | gone.png |

  Rule: The reader is chosen by the file's extension

    Scenario: The extension's case does not matter
      Given the text file "LOUD.OBJ":
        """
        v -50 -50 0
        v 50 -50 0
        v 0 50 0
        f 1 2 3
        """
      And the QML:
        """qml
        Shown { source: "LOUD.OBJ" }
        """
      When 100 milliseconds pass
      Then the property "status" of "loader" is 1

    Scenario: An unknown extension is an error that says so
      Given the text file "model.fbx":
        """
        v 0 0 0
        """
      And the QML:
        """qml
        import QtQuick3D
        Shown {
          objectName: "root"; source: "model.fbx"
          property bool says: loader.errorString.indexOf("model.fbx") >= 0
        }
        """
      When 100 milliseconds pass
      Then the property "status" of "loader" is 2
      And the property "says" of "root" is true

    Scenario Outline: <what> is an error
      Given the text file "broken.obj":
        """
        v 1 2
        f 1 2 3
        """
      And the text file "empty.obj":
        """
        # only a comment
        """
      And the QML:
        """qml
        import QtQuick3D
        Shown {
          objectName: "root"; source: "<file>"
          property bool says: loader.errorString.indexOf("<file>") >= 0
        }
        """
      When 100 milliseconds pass
      Then the property "status" of "loader" is 2
      And the property "says" of "root" is true

      Examples:
        | what                       | file        |
        | A file that is not there   | nowhere.obj |
        | A vertex that is cut short | broken.obj  |
        | A file with no faces       | empty.obj   |

  Rule: A loaded file works as any loaded model does

    Scenario: The bounds are the box the faces fill
      Given the QML:
        """qml
        Shown { source: "groups.obj" }
        """
      When 100 milliseconds pass
      Then the property "bounds.minimum.x" of "loader" is within 0.001 of -70
      And the property "bounds.maximum.x" of "loader" is within 0.001 of 70
      And the property "bounds.minimum.y" of "loader" is within 0.001 of -20
      And the property "bounds.maximum.y" of "loader" is within 0.001 of 20

    Scenario Outline: A pick names the file's <name> object
      Given the QML:
        """qml
        import QtQuick
        Shown {
          objectName: "root"; source: "groups.obj"
          property string hit: ""
          function pickAt(x, y) {
            let result = view.pick(x, y)
            hit = result.objectHit ? result.objectHit.objectName : "nothing"
          }
        }
        """
      When 100 milliseconds pass
      And the method "pickAt" of "root" is called with <x>, 100
      Then the property "hit" of "root" is "<name>"

      Examples:
        | name  | x   |
        | left  | 50  |
        | right | 150 |

    Scenario: Changing the source from a glTF file to an OBJ file
      Given the model file "square.gltf"
      And the QML:
        """qml
        Shown { source: "square.gltf" }
        """
      When 100 milliseconds pass
      And the property "source" of "loader" is set to "groups.obj"
      And 100 milliseconds pass
      Then the property "status" of "loader" is 1
      And the pixel at 100, 100 of the window is "#202020"
      And the pixel at 50, 100 of the window is within 2 of "#00ff00"

  @ljos-only
  Rule: A loaded model costs nothing while it is still

    Scenario: Once loaded, nothing is scheduled
      Given the QML:
        """qml
        Shown { source: "picture.obj" }
        """
      When 100 milliseconds pass
      Then the property "status" of "loader" is 1
      And the application has nothing scheduled
