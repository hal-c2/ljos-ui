import QtQuick
import QtQuick3D
import QtQuick3D.Helpers

// A panel of how the scene is being drawn: frame rate and time, the GPU's
// time where the host measures it and, with resourceDetailsVisible, the
// draw calls and shadow passes.
Rectangle {
    id: root

    // The view the numbers are about. The renderer keeps one record for
    // the process, so it only decides whether the panel refreshes.
    property View3D source: null
    property bool resourceDetailsVisible: false

    color: Qt.rgba(0, 0, 0, 0.6)
    implicitWidth: column.implicitWidth + 16
    implicitHeight: column.implicitHeight + 16

    RenderStatistics { id: stats }

    // Only while the panel is shown: a hidden one wakes nothing. The
    // numbers are copied, so a scene that is not drawn changes none of
    // them and nothing is painted.
    Timer {
        interval: 500
        repeat: true
        running: root.visible && root.source !== null
        onTriggered: stats.refresh()
    }

    Column {
        id: column
        x: 8
        y: 8
        spacing: 2

        Text {
            color: "white"
            text: "FPS: " + stats.fps.toFixed(0)
        }
        Text {
            color: "white"
            text: "Frame: " + stats.frameTime.toFixed(2) + " ms"
        }
        Text {
            visible: stats.gpuTime >= 0
            color: "white"
            text: "GPU: " + stats.gpuTime.toFixed(2) + " ms"
        }
        Text {
            visible: stats.gpuPostTime >= 0
            color: "white"
            text: "GPU post: " + stats.gpuPostTime.toFixed(2) + " ms"
        }
        Text {
            visible: root.resourceDetailsVisible
            color: "white"
            text: "Draw calls: " + stats.drawCalls
        }
        Text {
            visible: root.resourceDetailsVisible
            color: "white"
            text: "Shadow passes: " + stats.shadowPasses
        }
        Text {
            visible: root.resourceDetailsVisible
            color: "white"
            text: "Frames drawn: " + stats.frames
        }
    }
}
