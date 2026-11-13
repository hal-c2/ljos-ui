# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-renderstats.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-view3d.html#renderStats-prop
#   https://doc.qt.io/qt-6/qml-qtquick3d-helpers-debugview.html
#   docs/ARCHITECTURE.md section 2 (0 % CPU when idle)
# Qt fills renderStats from its own renderer; what it counts in a frame is
# its own, so only Ljós is held to these numbers. The names are Qt's where
# Qt has the property.
# That a still scene is not drawn again is in view3d.feature ("A still
# scene costs nothing"); every scenario here ticks a Timer to read the
# numbers, so none of them can be still.

@ljos-only
Feature: What drawing a 3D scene costs
  A View3D says what each frame of its scene cost: what was drawn and how
  often the drawing changed what it draws with, what was uploaded for it,
  and the time each part of the frame took on the CPU, from reading the
  scene to handing it over. The numbers are read when asked for, so a
  scene that is not drawn costs nothing to measure.

  Background:
    Given a 64 by 64 image file "checker.png" filled with "#3366cc"
    And the QML:
      """qml
      import QtQuick
      import QtQuick3D
      import QtQuick3D.Helpers
      Rectangle {
        width: 300; height: 200; color: "black"
        View3D {
          id: view
          anchors.fill: parent
          PerspectiveCamera { z: 400 }
          DirectionalLight { castsShadow: true }
          Model { source: "#Cube"; materials: DefaultMaterial {
              diffuseMap: Texture { source: "checker.png" } } }
          Model { source: "#Sphere"; x: 120
                  materials: PrincipledMaterial { baseColor: "red" } }
        }
        RenderStatistics { id: stats; objectName: "stats" }
        Timer { interval: 100; repeat: true; running: true; onTriggered: stats.refresh() }
      }
      """

  Rule: A frame says what it drew and what it changed to draw it

    Scenario: The models drawn and the passes they took are counted
      # The window draws a scene when asked for its pixels.
      When 100 milliseconds pass
      Then the pixel at 5, 195 of the window is not "#123456"
      When 200 milliseconds pass
      Then the property "drawCallCount" of "stats" is at least 2
      And the property "drawVertexCount" of "stats" is at least 36
      And the property "renderPassCount" of "stats" is at least 2
      And the property "pipelineSwitches" of "stats" is at least 2
      And the property "bindGroupSets" of "stats" is at least 4
      And the property "modelsDrawn" of "stats" is 2
      And the property "modelsCulled" of "stats" is 0

    Scenario: A model the camera does not see is left out
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        import QtQuick3D.Helpers
        Rectangle {
          width: 300; height: 200; color: "black"
          View3D {
            anchors.fill: parent
            PerspectiveCamera { z: 400; frustumCullingEnabled: true }
            DirectionalLight { }
            Model { source: "#Cube"; materials: DefaultMaterial { } }
            Model { source: "#Cube"; z: 1000; materials: DefaultMaterial { } }
          }
          RenderStatistics { id: stats; objectName: "stats" }
          Timer { interval: 100; repeat: true; running: true; onTriggered: stats.refresh() }
        }
        """
      When 100 milliseconds pass
      Then the pixel at 5, 195 of the window is not "#123456"
      When 200 milliseconds pass
      Then the property "modelsDrawn" of "stats" is 1
      And the property "modelsCulled" of "stats" is 1

  Rule: A frame says what was uploaded and made for it

    Scenario: The first frame uploads its texture and its uniforms
      # 64 by 64 texels of four bytes. The pipelines it draws with are made
      # once for the device, which a scenario before this one may have done,
      # so they are not asked for here.
      When 100 milliseconds pass
      Then the pixel at 5, 195 of the window is not "#123456"
      When 200 milliseconds pass
      Then the property "bufferWrites" of "stats" is at least 1
      And the property "bufferWriteBytes" of "stats" is at least 64
      And the property "textureUploads" of "stats" is at least 1
      And the property "textureUploadBytes" of "stats" is at least 16384

  Rule: A frame says how long each part of it took on the CPU

    Scenario: Reading the scene, preparing, recording and handing it over are timed apart
      When 100 milliseconds pass
      Then the pixel at 5, 195 of the window is not "#123456"
      When 200 milliseconds pass
      Then the property "syncTime" of "stats" is at least 0
      And the property "renderPrepareTime" of "stats" is at least 0
      And the property "encodeTime" of "stats" is at least 0
      And the property "submitTime" of "stats" is at least 0
      And the property "renderTime" of "stats" is at least 0

    Scenario: A view's renderStats has the counts under Qt's names
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Rectangle {
          width: 300; height: 200; color: "black"
          View3D {
            id: view
            anchors.fill: parent
            PerspectiveCamera { z: 400 }
            DirectionalLight { }
            Model { source: "#Cube"; materials: DefaultMaterial { } }
          }
          Text {
            objectName: "readout"
            property int draws: view.renderStats.drawCallCount
            property int vertices: view.renderStats.drawVertexCount
            property int passes: view.renderStats.renderPassCount
            property int pipelines: view.renderStats.pipelineCount
            property real sync: view.renderStats.syncTime
            property real prepare: view.renderStats.renderPrepareTime
            property real render: view.renderStats.renderTime
          }
        }
        """
      When 100 milliseconds pass
      Then the pixel at 5, 195 of the window is not "#123456"
      When 1100 milliseconds pass
      Then the property "draws" of "readout" is at least 1
      And the property "vertices" of "readout" is at least 36
      And the property "passes" of "readout" is at least 1
      And the property "pipelines" of "readout" is at least 1
      And the property "sync" of "readout" is at least 0
      And the property "prepare" of "readout" is at least 0
      And the property "render" of "readout" is at least 0
      And the log has no warnings
