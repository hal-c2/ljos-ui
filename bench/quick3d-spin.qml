// A lit cube turning without end among 2D items: what drawing a 3D view
// costs in every frame.
import QtQuick
import QtQuick3D

Window {
    id: root
    width: 400
    height: 300
    visible: true
    color: "#2a2a40"

    property real angle: 0
    NumberAnimation on angle {
        from: 0
        to: 360
        duration: 4000
        loops: Animation.Infinite
    }

    Text {
        anchors.horizontalCenter: parent.horizontalCenter
        y: 16
        text: "A turning cube"
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
            eulerRotation: Qt.vector3d(20, root.angle, 0)
            materials: DefaultMaterial { diffuseColor: "#4080ff" }
        }
    }
}
