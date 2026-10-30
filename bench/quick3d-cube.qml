// A still 3D scene among 2D items: what a View3D costs to open, and that
// a scene nothing changes costs nothing once it is drawn.
import QtQuick
import QtQuick3D

Window {
    width: 400
    height: 300
    visible: true
    color: "#2a2a40"

    Text {
        anchors.horizontalCenter: parent.horizontalCenter
        y: 16
        text: "A lit cube"
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
        PerspectiveCamera { z: 300 }
        DirectionalLight { eulerRotation.x: -30 }
        Model {
            source: "#Cube"
            eulerRotation: Qt.vector3d(20, 30, 0)
            materials: DefaultMaterial { diffuseColor: "#4080ff" }
        }
    }
}
