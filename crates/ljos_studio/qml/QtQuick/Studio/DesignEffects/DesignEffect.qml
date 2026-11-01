import QtQuick

// Sits behind what its parent draws. Of the effects Design Studio exports,
// this draws the background blur: what is behind the parent in
// backgroundLayer, blurred. The other effects are accepted and not drawn.
Item {
    id: root

    property Item backgroundLayer: null
    property real backgroundBlurRadius: 0
    property bool backgroundBlurVisible: true
    property real layerBlurRadius: 0
    property bool layerBlurVisible: true
    property list<QtObject> effects

    readonly property bool blurs: backgroundBlurVisible && backgroundBlurRadius > 0
                                  && backgroundLayer !== null && parent !== null
    property rect backgroundRect

    function place() {
        if (!blurs)
            return
        const at = mapToItem(backgroundLayer, 0, 0)
        backgroundRect = Qt.rect(at.x, at.y, width, height)
        behind.scheduleUpdate()
    }

    anchors.fill: parent
    z: -1

    Component.onCompleted: place()
    onBlursChanged: place()
    onWidthChanged: place()
    onHeightChanged: place()
    Connections {
        target: root.parent
        function onXChanged() { root.place() }
        function onYChanged() { root.place() }
    }

    // A copy this small, smoothed up to the item's size, spreads each of its
    // pixels over half the radius: no real blur's kernel. It is taken when
    // the item moves and not every frame, as a copy of a 3D view draws the
    // scene again: three buttons on one tripled what a frame took.
    ShaderEffectSource {
        id: behind
        anchors.fill: parent
        live: false
        visible: root.blurs
        sourceItem: root.blurs ? root.backgroundLayer : null
        sourceRect: root.backgroundRect
        smooth: true
        readonly property real step: Math.max(1, root.backgroundBlurRadius / 2)
        textureSize: Qt.size(Math.max(1, Math.ceil(root.width / step)),
                             Math.max(1, Math.ceil(root.height / step)))
    }
}
