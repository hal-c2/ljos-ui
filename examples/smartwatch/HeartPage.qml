import Ljos.Ui
import Ljos.Layout
import Ljos.Core

// The heart, sampled once a second while this screen is shown, and the
// day's three rings.
WatchScreen {
    id: page

    Title {
        theme: page.theme
        y: page.round ? 50 : 24
        text: t`HEART`
    }

    Pulse {
        id: pulse
        x: page.inset + 4
        y: page.round ? 96 : 76
        width: page.width - 2 * (page.inset + 4)
        chartHeight: page.round ? 104 : 150
        theme: page.theme
        watch: page.watch
    }

    Row {
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: page.round ? 44 : 30
        spacing: 18
        Repeater {
            model: 3
            delegate: Column {
                id: cell
                required property int index
                spacing: 4
                Item {
                    Layout.alignment: Align.HCenter
                    width: 44
                    height: 44
                    Ring {
                        anchors.fill: parent
                        theme: page.theme
                        thickness: 6
                        value: cell.index === 0 ? page.watch.rings.move
                            : cell.index === 1 ? page.watch.rings.exercise : page.watch.rings.stand
                        tint: cell.index === 0 ? page.theme.magenta
                            : cell.index === 1 ? page.theme.green : page.theme.cyan
                    }
                }
                Text {
                    Layout.alignment: Align.HCenter
                    text: cell.index === 0 ? t`Move` : cell.index === 1 ? t`Exercise` : t`Stand`
                    color: page.theme.muted
                    font.size: 13
                }
            }
        }
    }
}
