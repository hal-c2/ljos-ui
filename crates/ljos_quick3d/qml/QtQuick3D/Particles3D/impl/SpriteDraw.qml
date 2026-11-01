import QtQuick3D

// What draws a kind of sprite or line: one rectangle, once for each row of the
// table its kind fills. The rows say everything that differs between them.
Model {
    id: draw
    property bool line: false
    property int blendMode: 0
    property alias sprite: picture.texture
    property alias colorTable: table.texture
    property bool pictured: false
    property bool tabled: false
    property real frames: 1
    property real interpolate: 0
    property bool billboard: false

    source: "#Rectangle"
    castsShadows: false
    receivesShadows: false
    pickable: false
    materials: CustomMaterial {
        shadingMode: CustomMaterial.Unshaded
        depthDrawMode: Material.NeverDepthDraw
        cullMode: Material.NoCulling
        // SourceOver, Screen, Multiply. The shader premultiplies: one factor
        // does for colour and alpha, and only this way what was opaque stays so.
        sourceBlend: draw.blendMode === 2 ? CustomMaterial.DstColor : CustomMaterial.One
        destinationBlend: draw.blendMode === 0 ? CustomMaterial.OneMinusSrcAlpha
                        : draw.blendMode === 1 ? CustomMaterial.One : CustomMaterial.Zero
        vertexShader: draw.line ? "ljos-quick3d-particles:/line.vert" : "ljos-quick3d-particles:/sprite.vert"
        fragmentShader: "ljos-quick3d-particles:/sprite.frag"

        property TextureInput spritePicture: TextureInput { id: picture }
        property TextureInput spriteTable: TextureInput { id: table }
        property bool pictured: draw.pictured
        property bool tabled: draw.tabled
        property real frames: draw.frames
        property real interpolate: draw.interpolate
        property bool billboard: draw.billboard
        property bool multiply: draw.blendMode === 2
    }
}
