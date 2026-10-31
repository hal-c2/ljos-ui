import QtQuick3D

Effect {
    property real blurFalloff: 0
    property real bloomThreshold: 1
    property real channelThreshold: 1
    property real exposure: 0
    property real gamma: 1
    property real tonemappingLerp: 1

    Buffer {
        id: bright
        name: "bright"
        format: Buffer.RGBA16F
        sizeMultiplier: 0.5
    }
    Buffer {
        id: across
        name: "across"
        format: Buffer.RGBA16F
        sizeMultiplier: 0.5
    }
    Buffer {
        id: glow
        name: "glow"
        format: Buffer.RGBA16F
        sizeMultiplier: 0.5
    }

    passes: [
        Pass {
            output: bright
            shaders: Shader {
                stage: Shader.Fragment
                shader: "ljos-quick3d-effects:/hdr-bright.frag"
            }
        },
        Pass {
            output: across
            commands: BufferInput {
                buffer: bright
            }
            shaders: Shader {
                stage: Shader.Fragment
                shader: "ljos-quick3d-effects:/hdr-blur-x.frag"
            }
        },
        Pass {
            output: glow
            commands: BufferInput {
                buffer: across
            }
            shaders: Shader {
                stage: Shader.Fragment
                shader: "ljos-quick3d-effects:/hdr-blur-y.frag"
            }
        },
        Pass {
            commands: BufferInput {
                buffer: glow
                sampler: "glowTexture"
            }
            shaders: Shader {
                stage: Shader.Fragment
                shader: "ljos-quick3d-effects:/hdr-combine.frag"
            }
        }
    ]
}
