import Ljos.Ui
import Ljos.Core
import Ljos.Design
import "logic/watch.ts" as W
import "logic/dial.ts" as Dial

// Every app, six round the ring (from just past twelve) and one in the
// middle. The crown walks
// them in order and Return opens the one it is on; a tap opens any.
WatchScreen {
    id: launcher

    readonly property int current: launcher.watch.choice
    readonly property real spread: Math.min(launcher.width, launcher.height) / 2 - 92

    Repeater {
        model: W.apps
        delegate: Item {
            id: cell
            required property W.App modelData
            required property int index
            readonly property bool middle: cell.index === W.apps.length - 1
            readonly property Dial.Spot spot: cell.middle ? { x: 0, y: 0 }
                : Dial.around(cell.index + 0.5, W.apps.length - 1, launcher.spread)
            readonly property bool chosen: cell.index === launcher.current
            x: launcher.width / 2 + cell.spot.x - width / 2
            y: launcher.height / 2 + cell.spot.y - height / 2
            width: 84
            height: 84
            scale: cell.chosen ? (Motion.reduced ? 1.04 : 1.12) : 1
            Behavior on scale {
                NumberAnimation {
                    duration: Motion.moveSmall
                    easing.type: Easing.OutCubic
                }
            }
            Chip {
                anchors.fill: parent
                theme: launcher.theme
                glyph: cell.modelData.glyph
                glyphSize: 34
                help: launcher.watch.title(cell.modelData.page)
                active: cell.chosen
                tint: cell.modelData.page === "health" || cell.modelData.page === "music" ? launcher.theme.magenta
                    : cell.modelData.page === "workout" ? launcher.theme.green
                    : launcher.theme.cyan
                onPressed: launcher.watch.show(cell.modelData.page)
            }
        }
    }

    Text {
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: launcher.round ? 40 : 30
        text: `${launcher.watch.title(W.apps[launcher.current]?.page ?? "launcher")}`
        color: launcher.theme.text
        font.size: 15
        font.weight: Font.DemiBold
        font.letterSpacing: 1.5
    }
}
