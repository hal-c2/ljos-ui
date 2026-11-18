// This car on the road, as the cluster shows it: the same CarModel the car
// panel has, seen from behind and above, so the tail lights face the
// driver the way the lane does. Nothing moves here unless the car does, so
// it draws only when the drive changes it.
//
// This file stays in the QtQuick3D vocabulary: Ljós runs it as it is.
import QtQuick
import QtQuick3D
import QtQuick3D.Helpers

View3D {
    id: chase

    property bool bloom: true
    property bool dark: true
    property color stripTint: "#00e5ff"
    property color tailTint: "#ff0d59"
    // A turn of the whole car, in degrees, as it drifts across the lane. The
    // wheels stay still: the drive steps once a second, too coarse to turn
    // them by.
    property real sway: 0

    readonly property real m: 100

    environment: ExtendedSceneEnvironment {
        backgroundMode: SceneEnvironment.Transparent
        antialiasingMode: SceneEnvironment.MSAA
        antialiasingQuality: SceneEnvironment.High
        tonemapMode: SceneEnvironment.TonemapModeFilmic
        glowEnabled: chase.bloom
        glowStrength: 1.1
        glowIntensity: chase.dark ? 0.9 : 0.5
        glowBloom: 0.25
        glowHDRMinimumValue: 1.2
        glowLevel: ExtendedSceneEnvironment.GlowLevel.Two
            | ExtendedSceneEnvironment.GlowLevel.Three
    }

    // Behind the car (its nose is +z), above it, looking down the road.
    Node {
        y: 0.5 * chase.m
        eulerRotation.y: 180
        Node {
            eulerRotation.x: -17
            PerspectiveCamera {
                z: 7.8 * chase.m
                fieldOfView: 24
                clipNear: 10
                clipFar: 5000
            }
        }
    }

    DirectionalLight {
        eulerRotation.x: -50
        eulerRotation.y: 200
        brightness: chase.dark ? 2.2 : 1.5
    }
    DirectionalLight {
        eulerRotation.x: -20
        eulerRotation.y: 30
        color: chase.dark ? "#c04ab8" : "#ffffff"
        brightness: chase.dark ? 0.8 : 0.5
    }
    DirectionalLight {
        eulerRotation.x: -12
        eulerRotation.y: 180
        color: chase.dark ? "#6fa8c8" : "#ffffff"
        brightness: chase.dark ? 1.4 : 0.8
    }

    CarModel {
        eulerRotation.y: chase.sway
        stripTint: chase.stripTint
        tailTint: chase.tailTint
    }
}
