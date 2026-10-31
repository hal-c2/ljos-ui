import QtQuick3D

Effect {
    property real redStrength: 1
    property real greenStrength: 1.5
    property real blueStrength: 1
    property real saturation: 0

    passes: Pass {
        shaders: Shader {
            stage: Shader.Fragment
            shader: "ljos-quick3d-effects:/color-master.frag"
        }
    }
}
