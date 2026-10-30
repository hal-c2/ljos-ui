# Sources:
#   https://doc.qt.io/qt-6/quick3d-instancing.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-instancing.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-model.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation; each
# Rule is tagged as checked against Qt once qml6 has checked it.

Feature: Where instanced copies begin and the order they are drawn in
  By default a copy is the model shaped by its own scale and rotation, then
  placed by its entry; instanceRoot moves that split up the tree, so the
  nodes between the root and the model shape each copy too. Copies are
  drawn in the table's order unless depthSortingEnabled sorts them from the
  farthest from the camera; they are sorted only against each other.

  # An orthographic camera at magnification 1 shows one scene unit as one
  # pixel, so the view's centre is the scene's origin. Whether colours
  # blend in linear light or in sRGB is not documented, so each expected
  # colour takes either, as in transparency.feature.

  Rule: The instance root moves where the copies' own space begins

    # The small square is the big one's child, 100 of its units (20 scene
    # units) to its right. With the big square as its instance root each
    # of its copies keeps beside a copy of the big one.
    Scenario: A child keeps its place beside each copy of its instance root
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Rectangle {
          width: 200; height: 200; color: "black"
          View3D {
            anchors.fill: parent
            environment: SceneEnvironment { clearColor: "#202020"; backgroundMode: SceneEnvironment.Color }
            OrthographicCamera { z: 600 }
            InstanceList {
              id: table
              InstanceListEntry { position: Qt.vector3d(-50, 0, 0) }
              InstanceListEntry { position: Qt.vector3d(50, 0, 0) }
            }
            Model {
              id: big; source: "#Rectangle"; scale: Qt.vector3d(0.2, 0.2, 0.2); instancing: table
              materials: DefaultMaterial { lighting: DefaultMaterial.NoLighting; diffuseColor: "red" }
              Model {
                source: "#Rectangle"; x: 100; z: 10; scale: Qt.vector3d(0.25, 0.25, 0.25)
                instancing: table; instanceRoot: big
                materials: DefaultMaterial { lighting: DefaultMaterial.NoLighting; diffuseColor: "lime" }
              }
            }
          }
        }
        """
      Then the pixel at 70, 100 of the window is within 2 of "#00ff00"
      And the pixel at 170, 100 of the window is within 2 of "#00ff00"
      And the pixel at 120, 100 of the window is "#202020"

  Rule: Depth sorting draws the copies from the farthest

    # Two half-transparent squares, one behind the other, the nearer blue
    # one first in the table.
    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Rectangle {
          width: 200; height: 200; color: "black"
          View3D {
            anchors.fill: parent
            environment: SceneEnvironment { clearColor: "black"; backgroundMode: SceneEnvironment.Color }
            OrthographicCamera { z: 600 }
            Model {
              source: "#Rectangle"; scale: Qt.vector3d(0.2, 0.2, 0.2)
              materials: DefaultMaterial { lighting: DefaultMaterial.NoLighting; diffuseColor: "white" }
              instancing: InstanceList {
                objectName: "table"; hasTransparency: true
                InstanceListEntry { position: Qt.vector3d(0, 0, 50); color: "#800000ff" }
                InstanceListEntry { position: Qt.vector3d(0, 0, -50); color: "#80ff0000" }
              }
            }
          }
        }
        """

    Scenario: Unsorted copies are drawn in the table's order
      Then the pixel at 100, 100 of the window is within 40 of "#9e0066"

    Scenario: Sorted copies lay the nearer over the farther
      When the property "depthSortingEnabled" of "table" is set to true
      Then the pixel at 100, 100 of the window is within 40 of "#66009e"
