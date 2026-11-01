# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-helpers-debugview.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation.

@ljos-only
Feature: A panel of how the scene is drawn
  A DebugView is a panel over a View3D that shows its frame rate and time,
  and with resourceDetailsVisible the number of drawings and the shadow passes.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick3D
      import QtQuick3D.Helpers
      Rectangle {
        width: 300; height: 200; color: "black"
        View3D {
          id: view
          anchors.fill: parent
          OrthographicCamera { z: 300 }
          DirectionalLight { }
          Model { source: "#Cube"; materials: DefaultMaterial { } }
        }
        RenderStatistics { id: stats; objectName: "stats" }
        Timer { interval: 100; repeat: true; running: true; onTriggered: stats.refresh() }
        DebugView {
          objectName: "debug"; source: view
          resourceDetailsVisible: true
        }
      }
      """

  Scenario: The panel is as large as what it shows
    Then the property "width" of "debug" is at least 60
    And the property "height" of "debug" is at least 40

  Scenario: What was drawn is counted
    # The window draws a scene when asked for its pixels.
    When 1 second passes
    Then the pixel at 5, 195 of the window is not "#123456"
    When 1 second passes
    Then the property "drawCalls" of "stats" is at least 1
    And the property "frames" of "stats" is at least 1
    And the property "gpuTime" of "stats" is -1

  Scenario: The time of the passes after the scene is apart from the GPU time
    # gpuTime holds the post-processing passes too and gpuPostTime those
    # alone. Both stay -1 where the host does not measure them, and
    # gpuPostTime stays -1 for a scene with no post-processing.
    When 1 second passes
    Then the property "gpuTime" of "stats" is -1
    And the property "gpuPostTime" of "stats" is -1

  Scenario: A scene that is not drawn any more has no frame rate
    When 2 seconds pass
    Then the property "fps" of "stats" is 0

  Scenario: A view has its numbers on renderStats, made when they are read
    Given the QML:
      """qml
      import QtQuick
      import QtQuick3D
      Rectangle {
        width: 300; height: 200; color: "black"
        View3D {
          id: view
          anchors.fill: parent
          OrthographicCamera { z: 300 }
          DirectionalLight { }
          Model { source: "#Cube"; materials: DefaultMaterial { } }
        }
        Text {
          objectName: "readout"
          property int fps: view.renderStats.fps
          property real frame: view.renderStats.frameTime
          property real longest: view.renderStats.maxFrameTime
          property real render: view.renderStats.renderTime
          property real sync: view.renderStats.syncTime
        }
      }
      """
    # The window draws a scene when asked for its pixels.
    When 100 milliseconds pass
    Then the pixel at 5, 195 of the window is not "#123456"
    When 1100 milliseconds pass
    Then the property "fps" of "readout" is at least 1
    And the property "frame" of "readout" is at least 0
    And the property "longest" of "readout" is at least 0
    And the property "render" of "readout" is -1
    And the property "sync" of "readout" is -1
    And the log has no warnings
