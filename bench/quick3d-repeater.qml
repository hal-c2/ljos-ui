// A Repeater3D that gains a delegate every 16 ms, from 500 cubes to 1000
// and back to 500: what a frame costs when what the scene is made of
// changes all the time, not only where its nodes are.
import QtQuick
import QtQuick3D

Window {
    id: root
    width: 640
    height: 480
    visible: true
    color: "#202030"

    ListModel { id: cubes }

    function fill(n) {
        for (let i = cubes.count; i < n; i++)
            cubes.append({ "slot": i })
    }

    Component.onCompleted: fill(500)

    Timer {
        interval: 16
        repeat: true
        running: true
        onTriggered: {
            if (cubes.count >= 1000)
                cubes.remove(500, cubes.count - 500)
            else
                root.fill(cubes.count + 1)
        }
    }

    View3D {
        anchors.fill: parent
        environment: SceneEnvironment {
            clearColor: "#202030"
            backgroundMode: SceneEnvironment.Color
        }
        PerspectiveCamera { y: 500; z: 900; eulerRotation.x: -30 }
        DirectionalLight { eulerRotation.x: -45; brightness: 0.6 }
        PointLight { y: 300; color: "#ffd0a0" }
        // 40 columns by 25 rows, 30 units apart, filled row by row.
        Repeater3D {
            model: cubes
            Model {
                source: "#Cube"
                x: (slot % 40 - 19.5) * 30
                z: (Math.floor(slot / 40) - 12) * 30
                scale: Qt.vector3d(0.2, 0.2, 0.2)
                materials: DefaultMaterial { diffuseColor: Qt.hsla(slot / 1000, 0.6, 0.55, 1) }
            }
        }
    }
}
