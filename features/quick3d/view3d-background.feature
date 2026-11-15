# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-sceneenvironment.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-view3d.html
# Measured on qml6 6.12.0 in a window. Not tagged @qt-parity: the nightly
# oracle has no QtQuick3D.

Feature: What a View3D shows behind its scene
  The SceneEnvironment's backgroundMode says what a View3D fills where no
  model is drawn. With SkyBox it is the light probe, seen through the
  camera, so a view with no camera has no sky to show and the items under
  it show through. A sky box with no picture to show, a SkyBox with no
  lightProbe or a SkyBoxCubeMap with no skyBoxCubeMap, fills the view with
  the clear colour instead.

  # The probe is an ordinary picture of #808080, which the sky box shows as
  # #7a7a7a. The window is #202020; the view is the 100 by 100 square at 50,
  # 50, its clear colour red.
  Background:
    Given a 64 by 32 image file "grey.png" filled with "#808080"

  Rule: A view with no camera shows no sky box

    Scenario: The items under a sky box no camera looks at show through
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Rectangle {
          width: 200; height: 200; color: "#202020"
          View3D {
            x: 50; y: 50; width: 100; height: 100
            environment: SceneEnvironment {
              backgroundMode: SceneEnvironment.SkyBox; clearColor: "#ff0000"
              lightProbe: Texture { source: "grey.png" }
            }
          }
        }
        """
      Then the pixel at 100, 100 of the window is within 2 of "#202020"
      And the pixel at 55, 55 of the window is within 2 of "#202020"

    Scenario: A camera shows the sky box
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Rectangle {
          width: 200; height: 200; color: "#202020"
          View3D {
            x: 50; y: 50; width: 100; height: 100
            environment: SceneEnvironment {
              backgroundMode: SceneEnvironment.SkyBox; clearColor: "#ff0000"
              lightProbe: Texture { source: "grey.png" }
            }
            PerspectiveCamera { z: 300 }
          }
        }
        """
      Then the pixel at 100, 100 of the window is within 3 of "#7a7a7a"
      And the pixel at 20, 20 of the window is within 2 of "#202020"

    Scenario: A clear colour fills a view with no camera
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Rectangle {
          width: 200; height: 200; color: "#202020"
          View3D {
            x: 50; y: 50; width: 100; height: 100
            environment: SceneEnvironment {
              backgroundMode: SceneEnvironment.Color; clearColor: "#ff0000"
            }
          }
        }
        """
      Then the pixel at 100, 100 of the window is within 2 of "#ff0000"

    # A View3D inside a Node is drawn in the outer scene, its top-left
    # corner at the origin in the middle of the window. The outer clear
    # colour #202020 is drawn as #1e1e1e.
    Scenario: A view with no camera drawn in another scene shows that scene
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
            OrthographicCamera { z: 600 }
            Node {
              View3D {
                width: 100; height: 100
                environment: SceneEnvironment {
                  backgroundMode: SceneEnvironment.SkyBox
                  lightProbe: Texture { source: "grey.png" }
                }
              }
            }
          }
        }
        """
      Then the pixel at 150, 150 of the window is within 2 of "#1e1e1e"
      And the pixel at 50, 50 of the window is within 2 of "#1e1e1e"

  Rule: A sky box with no picture to show is the clear colour

    Scenario Outline: <background> with nothing set, <camera>
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Rectangle {
          width: 200; height: 200; color: "#202020"
          View3D {
            x: 50; y: 50; width: 100; height: 100
            environment: SceneEnvironment {
              backgroundMode: SceneEnvironment.<background>; clearColor: "#ff0000"
            }
            <lens>
          }
        }
        """
      Then the pixel at 100, 100 of the window is within 2 of "#ff0000"
      And the pixel at 20, 20 of the window is within 2 of "#202020"

      Examples:
        | background    | camera           | lens                         |
        | SkyBox        | through a camera | PerspectiveCamera { z: 300 } |
        | SkyBox        | with no camera   |                              |
        | SkyBoxCubeMap | through a camera | PerspectiveCamera { z: 300 } |
