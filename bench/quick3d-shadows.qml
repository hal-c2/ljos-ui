// A cube turning over a floor, shadowed by a sun, a spot light and a
// point light: what drawing the shadow maps costs in every frame, since
// the caster moves and every map is drawn again.
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
        text: "Shadows of a turning cube"
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
        PerspectiveCamera {
            y: 200
            z: 400
            eulerRotation.x: -25
        }
        DirectionalLight {
            eulerRotation: Qt.vector3d(-60, 30, 0)
            brightness: 0.6
            castsShadow: true
            shadowMapQuality: Light.ShadowMapQualityHigh
        }
        SpotLight {
            x: -150
            y: 250
            z: 100
            eulerRotation: Qt.vector3d(-60, -40, 0)
            coneAngle: 60
            quadraticFade: 0
            brightness: 0.5
            color: "#ffd0a0"
            castsShadow: true
        }
        PointLight {
            x: 150
            y: 150
            z: 50
            quadraticFade: 0
            brightness: 0.4
            color: "#a0c0ff"
            castsShadow: true
        }
        Model {
            source: "#Rectangle"
            eulerRotation.x: -90
            y: -50
            scale: Qt.vector3d(8, 8, 1)
            materials: DefaultMaterial { diffuseColor: "#e0e0e0" }
        }
        Model {
            source: "#Cube"
            y: 30
            eulerRotation: Qt.vector3d(20, root.angle, 0)
            materials: DefaultMaterial { diffuseColor: "#4080ff" }
        }
        Model {
            source: "#Sphere"
            x: -120
            y: -10
            scale: Qt.vector3d(0.8, 0.8, 0.8)
            materials: DefaultMaterial { diffuseColor: "#ff8040" }
        }
    }
}
