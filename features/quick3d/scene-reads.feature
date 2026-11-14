# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-view3d.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-renderstats.html
#   docs/ARCHITECTURE.md section 2 (0 % CPU when idle)
# objectsRead is Ljós's own number: Qt has no property for what its scene
# graph reads again, so only Ljós is held to it.
# One scene to a file: a Background's window, drawn whenever this one is,
# would count what this scene read as its own.

@ljos-only
Feature: A View3D reads again only what changed
  A View3D keeps the scene it drew and patches it where its objects
  change. After one object's property changes, the view reads that
  object again, not the others in the scene, and RenderStatistics says
  how many objects it read as objectsRead.

  Rule: A change to one object reads only that object again

    Scenario: One object that moves is read alone, not the scene it is in
      # objectsRead counts what the view read the last time it was
      # drawn, which here is when the window's pixels are looked at, so
      # each count is checked before the pixels are looked at again. The
      # cubes, their materials and the light are not read again for the
      # one that moved. The sphere is unlit red, about 17 pixels across
      # its radius: 200 units to the right it is 69 pixels to the right,
      # clear of where it was and of the cubes.
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
              materials: DefaultMaterial { diffuseColor: "red"; lighting: DefaultMaterial.NoLighting }
            }
          }
          RenderStatistics { id: stats; objectName: "stats" }
          Timer { interval: 100; repeat: true; running: true; onTriggered: stats.refresh() }
        }
        """
      When 100 milliseconds pass
      Then the pixel at 150, 100 of the window is within 8 of "#ff0000"
      When 200 milliseconds pass
      Then the property "objectsRead" of "stats" is at least 100
      And the pixel at 219, 100 of the window is within 8 of "#000000"
      When the property "x" of "mover" is set to 200
      And 100 milliseconds pass
      Then the pixel at 219, 100 of the window is within 8 of "#ff0000"
      When 200 milliseconds pass
      Then the property "objectsRead" of "stats" is 1
      And the pixel at 150, 100 of the window is within 8 of "#000000"
      And the log has no warnings

  Rule: A node that leaves the view leaves the picture it patches

    Scenario: A model moved to a node outside the view is no longer drawn
      # The view's own lists say nothing when one leaves them, as Qt's
      # have no change signal; the model's parent does. The cube fills
      # the middle of the window until it is moved out.
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Rectangle {
          objectName: "root"; width: 100; height: 100; color: "black"
          Node { id: away }
          View3D {
            anchors.fill: parent
            PerspectiveCamera { z: 300 }
            Model {
              id: cube; source: "#Cube"
              materials: DefaultMaterial { diffuseColor: "red"; lighting: DefaultMaterial.NoLighting }
            }
          }
          function moveCubeAway() { cube.parent = away }
        }
        """
      Then the pixel at 50, 50 of the window is within 2 of "#ff0000"
      When the method "moveCubeAway" of "root" is called
      Then the pixel at 50, 50 of the window is within 2 of "#000000"
      And the log has no warnings
