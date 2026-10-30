// A thousand cubes made by a Repeater3D, under four lights of which one
// circles the grid: what a large still scene costs when only its light
// moves.
import QtQuick
import QtQuick3D

Window {
    id: root
    width: 640
    height: 480
    visible: true
    color: "#202030"

    property real angle: 0
    NumberAnimation on angle {
        from: 0
        to: 360
        duration: 4000
        loops: Animation.Infinite
    }

    View3D {
        anchors.fill: parent
        environment: SceneEnvironment {
            clearColor: "#202030"
            backgroundMode: SceneEnvironment.Color
        }
        PerspectiveCamera { y: 500; z: 900; eulerRotation.x: -30 }
        DirectionalLight { eulerRotation.x: -45; brightness: 0.6 }
        DirectionalLight { eulerRotation.y: 120; brightness: 0.3 }
        PointLight { y: 300; color: "#ffd0a0" }
        PointLight {
            x: 600 * Math.cos(root.angle * Math.PI / 180)
            z: 600 * Math.sin(root.angle * Math.PI / 180)
            y: 150
            color: "#80c0ff"
        }
        // 40 columns by 25 rows, 30 units apart.
        Repeater3D {
            model: 1000
            Model {
                source: "#Cube"
                x: (index % 40 - 19.5) * 30
                z: (Math.floor(index / 40) - 12) * 30
                scale: Qt.vector3d(0.2, 0.2, 0.2)
                materials: DefaultMaterial { diffuseColor: Qt.hsla(index / 1000, 0.6, 0.55, 1) }
            }
        }
    }
}
