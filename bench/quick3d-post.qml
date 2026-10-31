// A turning cube on a floor in an ExtendedSceneEnvironment with glow,
// depth of field, ambient occlusion and FXAA: what post-processing costs
// on the GPU every frame.
import QtQuick
import QtQuick3D
import QtQuick3D.Helpers

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

    View3D {
        anchors.fill: parent
        environment: ExtendedSceneEnvironment {
            clearColor: "#202030"
            backgroundMode: SceneEnvironment.Color
            tonemapMode: SceneEnvironment.TonemapModeFilmic
            glowEnabled: true
            glowBloom: 0.5
            glowHDRMinimumValue: 0.5
            glowLevel: ExtendedSceneEnvironment.GlowLevel.One | ExtendedSceneEnvironment.GlowLevel.Three | ExtendedSceneEnvironment.GlowLevel.Five
            depthOfFieldEnabled: true
            depthOfFieldFocusDistance: 300
            depthOfFieldFocusRange: 50
            depthOfFieldBlurAmount: 4
            aoStrength: 100
            aoDistance: 20
            aoSoftness: 50
            fxaaEnabled: true
        }
        PerspectiveCamera { z: 300; y: 60; eulerRotation.x: -10 }
        DirectionalLight { eulerRotation.x: -30; brightness: 2 }
        Model {
            source: "#Rectangle"
            y: -60
            scale: Qt.vector3d(6, 6, 1)
            eulerRotation.x: -90
            materials: DefaultMaterial { diffuseColor: "#808080" }
        }
        Model {
            source: "#Cube"
            eulerRotation: Qt.vector3d(20, root.angle, 0)
            materials: DefaultMaterial { diffuseColor: "#4080ff" }
        }
    }
}
