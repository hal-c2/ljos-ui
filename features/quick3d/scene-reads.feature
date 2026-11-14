# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-view3d.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-renderstats.html
#   docs/ARCHITECTURE.md section 2 (0 % CPU when idle)
# objectsRead is Ljós's own number: Qt has no property for what its scene
# graph reads again, so only Ljós is held to it.
# One scene to a file: a window of a Background drawn in the same frame
# would count what this scene read as its own.

@ljos-only
Feature: A View3D reads again only what changed
  A View3D keeps the scene it drew and patches it where its objects
  change. A frame after one object changed reads that object, not the
  others in the scene, and says how many it read as objectsRead.

  Rule: A frame reads again only the objects that changed

    Scenario: One object that moves is read alone, not the scene it is in
      # objectsRead is the last frame's. Here a frame is drawn when the
      # window's pixels are looked at, and what was read since the frame
      # before is its. The cubes, their materials and the light are not
      # read again for the one that moved.
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        import QtQuick3D.Helpers
        Rectangle {
          width: 300; height: 200; color: "black"
          View3D {
            anchors.fill: parent
            PerspectiveCamera { z: 600 }
            DirectionalLight { }
            Repeater3D {
              model: 100
              Model {
                source: "#Cube"; scale: Qt.vector3d(0.1, 0.1, 0.1)
                x: (index % 10) * 30 - 135; y: Math.floor(index / 10) * 15 - 70
                materials: DefaultMaterial { }
              }
            }
            Model {
              objectName: "mover"; source: "#Sphere"; z: 100
              materials: DefaultMaterial { diffuseColor: "red" }
            }
          }
          RenderStatistics { id: stats; objectName: "stats" }
          Timer { interval: 100; repeat: true; running: true; onTriggered: stats.refresh() }
        }
        """
      When 100 milliseconds pass
      Then the pixel at 5, 195 of the window is not "#123456"
      When 200 milliseconds pass
      Then the property "objectsRead" of "stats" is at least 100
      When the property "x" of "mover" is set to 40
      And 100 milliseconds pass
      Then the pixel at 5, 195 of the window is not "#123456"
      When 200 milliseconds pass
      Then the property "objectsRead" of "stats" is 1
      And the log has no warnings
