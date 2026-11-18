import Ljos.Ui
import QtQuick.Effects

// Bloom for what is lit: its children are drawn as they are and, with the
// theme's bloom on, again under themselves as a soft halo of `tint`. Only
// strokes and lamps go in here; text stays outside, crisp.
Item {
    id: neon

    required property Theme theme
    property color tint: neon.theme.cyan
    // 0 to 1: how far the halo reaches.
    property real spread: 1
    default property alias content: lit.data

    Item {
        id: lit
        anchors.fill: parent
        // With bloom on, the effect draws these: hidden here, they are not
        // drawn twice.
        visible: !neon.theme.bloom
    }

    MultiEffect {
        anchors.fill: lit
        source: lit
        visible: neon.theme.bloom
        shadowEnabled: true
        shadowColor: neon.tint
        shadowOpacity: neon.theme.glow
        shadowBlur: neon.spread
        shadowHorizontalOffset: 0
        shadowVerticalOffset: 0
        blurMax: 28
    }
}
