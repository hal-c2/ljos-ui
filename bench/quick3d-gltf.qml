// A glTF model read while the window opens: nine spheres of about 5000
// triangles from bench/quick3d/sphere-grid.glb (made by
// features/quick3d/models/gen.py). What a RuntimeLoader costs to open and
// to hold, and that a loaded model nothing changes costs nothing.
import QtQuick
import QtQuick3D

Window {
    width: 640
    height: 480
    visible: true
    color: "#202030"

    Text {
        anchors.horizontalCenter: parent.horizontalCenter
        y: 16
        text: loader.status === RuntimeLoader.Success ? "A glTF model"
            : loader.status === RuntimeLoader.Error ? loader.errorString : "Loading"
        color: "white"
        font.pixelSize: 20
    }

    View3D {
        anchors.fill: parent
        anchors.topMargin: 48
        environment: SceneEnvironment {
            clearColor: "#202030"
            backgroundMode: SceneEnvironment.Color
        }
        PerspectiveCamera { z: 500 }
        DirectionalLight { eulerRotation.x: -30; eulerRotation.y: -20 }
        RuntimeLoader {
            id: loader
            source: "quick3d/sphere-grid.glb"
        }
    }
}
