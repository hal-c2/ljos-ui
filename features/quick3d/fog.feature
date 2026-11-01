# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-fog.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-sceneenvironment.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation; each
# Rule is tagged as checked against Qt once qml6 has checked it.

Feature: Fog in a scene
  A SceneEnvironment's fog takes lit surfaces towards its colour, by
  their distance from the camera, by their height, or both; its density
  is the most of the colour there can be. The camera here is 600 units
  from a white surface lit to full white, and the fog's colour is black
  unless a scenario says otherwise.

  Rule: Fog is off until it is enabled

    Scenario: A fog not enabled
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Rectangle {
          width: 200; height: 200; color: "black"
          View3D {
            anchors.fill: parent
            environment: SceneEnvironment { clearColor: "#202020"; backgroundMode: SceneEnvironment.Color; fog: Fog { depthEnabled: true; depthNear: 0; depthFar: 100; color: "black" } }
            OrthographicCamera { z: 600 }
            DirectionalLight { }
            Model { source: "#Rectangle"; materials: PrincipledMaterial { specularAmount: 0; roughness: 1 } }
          }
        }
        """
      Then the pixel at 100, 100 of the window is within 4 of "#ffffff"

  Rule: Distance brings the fog on between its near and its far

    Scenario: Past its far, as dense as it gets
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Rectangle {
          width: 200; height: 200; color: "black"
          View3D {
            anchors.fill: parent
            environment: SceneEnvironment { clearColor: "#202020"; backgroundMode: SceneEnvironment.Color; fog: Fog { enabled: true; color: "red"; density: 1; depthEnabled: true; depthNear: 0; depthFar: 100 } }
            OrthographicCamera { z: 600 }
            DirectionalLight { }
            Model { source: "#Rectangle"; materials: PrincipledMaterial { specularAmount: 0; roughness: 1 } }
          }
        }
        """
      Then the pixel at 100, 100 of the window is within 4 of "#ff0000"

    Scenario: Density is how much of the colour
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Rectangle {
          width: 200; height: 200; color: "black"
          View3D {
            anchors.fill: parent
            environment: SceneEnvironment { clearColor: "#202020"; backgroundMode: SceneEnvironment.Color; fog: Fog { enabled: true; color: "black"; density: 0.5; depthEnabled: true; depthNear: 0; depthFar: 100 } }
            OrthographicCamera { z: 600 }
            DirectionalLight { }
            Model { source: "#Rectangle"; materials: PrincipledMaterial { specularAmount: 0; roughness: 1 } }
          }
        }
        """
      Then the pixel at 100, 100 of the window is within 4 of "#bcbcbc"

    Scenario: Before its near, none
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Rectangle {
          width: 200; height: 200; color: "black"
          View3D {
            anchors.fill: parent
            environment: SceneEnvironment { clearColor: "#202020"; backgroundMode: SceneEnvironment.Color; fog: Fog { enabled: true; color: "black"; density: 1; depthEnabled: true; depthNear: 1000; depthFar: 2000 } }
            OrthographicCamera { z: 600 }
            DirectionalLight { }
            Model { source: "#Rectangle"; materials: PrincipledMaterial { specularAmount: 0; roughness: 1 } }
          }
        }
        """
      Then the pixel at 100, 100 of the window is within 4 of "#ffffff"

    Scenario: Halfway, half
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Rectangle {
          width: 200; height: 200; color: "black"
          View3D {
            anchors.fill: parent
            environment: SceneEnvironment { clearColor: "#202020"; backgroundMode: SceneEnvironment.Color; fog: Fog { enabled: true; color: "black"; density: 1; depthEnabled: true; depthNear: 400; depthFar: 800 } }
            OrthographicCamera { z: 600 }
            DirectionalLight { }
            Model { source: "#Rectangle"; materials: PrincipledMaterial { specularAmount: 0; roughness: 1 } }
          }
        }
        """
      Then the pixel at 100, 100 of the window is within 4 of "#bcbcbc"

    Scenario: Transmission lets the surface's light through
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Rectangle {
          width: 200; height: 200; color: "black"
          View3D {
            anchors.fill: parent
            environment: SceneEnvironment { clearColor: "#202020"; backgroundMode: SceneEnvironment.Color; fog: Fog { enabled: true; color: "black"; density: 1; depthEnabled: true; depthNear: 400; depthFar: 800; transmitEnabled: true; transmitCurve: 1 } }
            OrthographicCamera { z: 600 }
            DirectionalLight { }
            Model { source: "#Rectangle"; materials: PrincipledMaterial { specularAmount: 0; roughness: 1 } }
          }
        }
        """
      Then the pixel at 100, 100 of the window is within 6 of "#e1e1e1"

  Rule: Height brings the fog on between its least and its most

    Scenario: Lower and higher
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Rectangle {
          width: 200; height: 200; color: "black"
          View3D {
            anchors.fill: parent
            environment: SceneEnvironment { clearColor: "#202020"; backgroundMode: SceneEnvironment.Color; fog: Fog { enabled: true; color: "black"; density: 1; heightEnabled: true; leastIntenseY: 0; mostIntenseY: 100; heightCurve: 1 } }
            OrthographicCamera { z: 600 }
            DirectionalLight { }
            Model { source: "#Rectangle"; materials: PrincipledMaterial { specularAmount: 0; roughness: 1 } }
          }
        }
        """
      Then the pixel at 100, 101 of the window is within 6 of "#ffffff"
      And the pixel at 100, 60 of the window is within 8 of "#d5d5d5"

  Rule: Changing the fog changes the picture

    Scenario: Its density is set
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Rectangle {
          width: 200; height: 200; color: "black"
          View3D {
            anchors.fill: parent
            environment: SceneEnvironment { clearColor: "#202020"; backgroundMode: SceneEnvironment.Color; fog: Fog { objectName: "fog"; enabled: true; color: "black"; density: 1; depthEnabled: true; depthNear: 0; depthFar: 100 } }
            OrthographicCamera { z: 600 }
            DirectionalLight { }
            Model { source: "#Rectangle"; materials: PrincipledMaterial { specularAmount: 0; roughness: 1 } }
          }
        }
        """
      Then the pixel at 100, 100 of the window is within 4 of "#000000"
      When the property "density" of "fog" is set to 0.5
      When 100 milliseconds pass
      Then the pixel at 100, 100 of the window is within 4 of "#bcbcbc"
