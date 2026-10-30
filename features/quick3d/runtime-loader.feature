# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-runtimeloader.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-node.html
#   https://doc.qt.io/qt-6/qtquick3d-coordinate-system.html
#   https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation; each
# Rule is tagged as checked against Qt once qml6 has checked it.

Feature: A glTF model loaded while the application runs
  A RuntimeLoader is a Node whose children come from a glTF file named by
  its source: the file's nodes with their transforms, its meshes and its
  materials. It says how the load went in status (Empty, Success or
  Error) and why it failed in errorString, and gives the box the model
  fills in its own space as bounds. The file is read in the background, so
  a large model does not stop the window.

  The model files are made by features/quick3d/models/gen.py:
  - square.gltf: one square 100 units across facing +Z, unlit red, its
    buffer inside the file;
  - pair.gltf with pair.bin: a square 40 units across, unlit green, 50
    units left (a matrix), and the same mesh unlit blue at half the size
    50 units right (translation and scale), under one parent node;
  - cube.glb: a lit cube 100 units across with normals and a texture,
    stretched to twice its height by its node.

  # An orthographic camera at magnification 1 shows one scene unit as one
  # pixel, so the view's centre is the scene's origin.
  Background:
    Given the model file "square.gltf"
    And the model file "pair.gltf"
    And the model file "pair.bin"
    And the model file "cube.glb"

  Rule: A new loader has loaded nothing

    Scenario: A RuntimeLoader with no source
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        View3D {
          width: 100; height: 100
          RuntimeLoader { objectName: "loader" }
        }
        """
      Then the property "status" of "loader" is 0
      And the property "errorString" of "loader" is ""
      And the property "source" of "loader" is ""

  Rule: The file's nodes are drawn under the loader

    Scenario: A square from a glTF file with its buffer inside
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
            RuntimeLoader { objectName: "loader"; source: "square.gltf" }
          }
        }
        """
      When 100 milliseconds pass
      Then the property "status" of "loader" is 1
      And the property "errorString" of "loader" is ""
      And the pixel at 100, 100 of the window is within 2 of "#ff0000"
      And the pixel at 60, 60 of the window is within 2 of "#ff0000"
      And the pixel at 40, 100 of the window is "#202020"

    Scenario: Nodes keep their parent, their matrix and their scale
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
            RuntimeLoader { objectName: "loader"; source: "pair.gltf" }
          }
        }
        """
      When 100 milliseconds pass
      Then the property "status" of "loader" is 1
      And the pixel at 50, 100 of the window is within 2 of "#00ff00"
      And the pixel at 35, 100 of the window is within 2 of "#00ff00"
      And the pixel at 150, 100 of the window is within 2 of "#0000ff"
      And the pixel at 165, 100 of the window is "#202020"
      And the pixel at 100, 100 of the window is "#202020"

    Scenario: A binary glTF file is lit by the scene's lights
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
            DirectionalLight {}
            RuntimeLoader { objectName: "loader"; source: "cube.glb"; scale: Qt.vector3d(0.5, 0.5, 0.5) }
          }
        }
        """
      When 100 milliseconds pass
      Then the property "status" of "loader" is 1
      And the pixel at 100, 100 of the window is not "#202020"
      And the pixel at 100, 135 of the window is not "#202020"
      And the pixel at 140, 100 of the window is "#202020"

  Rule: The loader is a node

    Scenario: Moving the loader moves the model
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
            RuntimeLoader { objectName: "loader"; source: "square.gltf"; x: 60 }
          }
        }
        """
      When 100 milliseconds pass
      Then the pixel at 140, 100 of the window is within 2 of "#ff0000"
      And the pixel at 60, 100 of the window is "#202020"

    Scenario: A hidden loader hides the model
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
            RuntimeLoader { objectName: "loader"; source: "square.gltf"; visible: false }
          }
        }
        """
      When 100 milliseconds pass
      Then the property "status" of "loader" is 1
      And the pixel at 100, 100 of the window is "#202020"

  Rule: Bounds are the box the model fills, in the loader's own space

    Scenario: The bounds follow the file's own node transforms
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        View3D {
          width: 100; height: 100
          RuntimeLoader { objectName: "loader"; source: "cube.glb"; x: 500 }
        }
        """
      When 100 milliseconds pass
      Then the property "bounds.minimum.x" of "loader" is within 0.001 of -50
      And the property "bounds.maximum.x" of "loader" is within 0.001 of 50
      And the property "bounds.minimum.y" of "loader" is within 0.001 of -100
      And the property "bounds.maximum.y" of "loader" is within 0.001 of 100
      And the property "bounds.maximum.z" of "loader" is within 0.001 of 50

    Scenario: Nothing loaded has empty bounds
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        View3D {
          width: 100; height: 100
          RuntimeLoader { objectName: "loader" }
        }
        """
      Then the property "bounds.minimum.x" of "loader" is 0
      And the property "bounds.maximum.y" of "loader" is 0

  Rule: Changing the source loads the new file

    Scenario: Another file replaces the first
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
            RuntimeLoader { objectName: "loader"; source: "square.gltf" }
          }
        }
        """
      When 100 milliseconds pass
      And the property "source" of "loader" is set to "pair.gltf"
      And 100 milliseconds pass
      Then the property "status" of "loader" is 1
      And the pixel at 100, 100 of the window is "#202020"
      And the pixel at 50, 100 of the window is within 2 of "#00ff00"

    Scenario: Taking the source away leaves nothing loaded
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
            RuntimeLoader { objectName: "loader"; source: "square.gltf" }
          }
        }
        """
      When 100 milliseconds pass
      And the property "source" of "loader" is set to ""
      Then the property "status" of "loader" is 0
      And the pixel at 100, 100 of the window is "#202020"
      And the property "bounds.maximum.x" of "loader" is 0

  Rule: A file that cannot be loaded is an error that says why

    Scenario Outline: <what> is an error
      Given the text file "broken.gltf":
        """
        {"asset": {"version": "2.0"}, "nodes": [
        """
      And the text file "lost.gltf":
        """
        {"asset": {"version": "2.0"}, "buffers": [{"uri": "gone.bin", "byteLength": 12}]}
        """
      And the QML:
        """qml
        import QtQuick
        import QtQuick3D
        View3D {
          objectName: "view"; width: 100; height: 100
          property bool says: loader.errorString.indexOf("<named>") >= 0
          RuntimeLoader { id: loader; objectName: "loader"; source: "<file>" }
        }
        """
      When 100 milliseconds pass
      Then the property "status" of "loader" is 2
      And the property "says" of "view" is true

      Examples:
        | what                       | file        | named       |
        | A file that is not there   | nowhere.glb | nowhere.glb |
        | A file that does not parse | broken.gltf | broken.gltf |
        | A buffer that is not there | lost.gltf   | gone.bin    |

    Scenario: A good file after a broken one loads
      Given the text file "broken.gltf":
        """
        {"asset":
        """
      And the QML:
        """qml
        import QtQuick
        import QtQuick3D
        View3D {
          width: 100; height: 100
          RuntimeLoader { objectName: "loader"; source: "broken.gltf" }
        }
        """
      When 100 milliseconds pass
      And the property "source" of "loader" is set to "square.gltf"
      And 100 milliseconds pass
      Then the property "status" of "loader" is 1
      And the property "errorString" of "loader" is ""

  @ljos-only
  Rule: A loaded model costs nothing while it is still

    Scenario: Once loaded, nothing is scheduled
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        View3D {
          width: 100; height: 100
          OrthographicCamera { z: 300 }
          RuntimeLoader { objectName: "loader"; source: "cube.glb" }
        }
        """
      When 100 milliseconds pass
      Then the property "status" of "loader" is 1
      And the application has nothing scheduled
