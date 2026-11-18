import Ljos.Ui
import Ljos.Layout
import Ljos.Core

// Pulled down from the face: the watch's modes as round switches, the
// battery above them.
WatchScreen {
    id: page

    Title {
        theme: page.theme
        y: page.round ? 58 : 30
        text: t`BATTERY ${page.watch.battery} %`
    }

    Rectangle {
        anchors.horizontalCenter: parent.horizontalCenter
        y: page.round ? 86 : 58
        width: 120
        height: 8
        radius: 4
        fill: page.theme.track
        Rectangle {
            width: parent.width * page.watch.battery / 100
            height: parent.height
            radius: 4
            fill: page.theme.green
        }
    }

    Row {
        anchors.centerIn: parent
        anchors.verticalCenterOffset: 20
        spacing: 6
        Switch {
            theme: page.theme
            glyph: "moon"
            label: t`Do not disturb`
            on: page.watch.quiet
            tint: page.theme.magenta
            onToggled: page.watch.quiet = !page.watch.quiet
        }
        Switch {
            theme: page.theme
            glyph: "sun"
            label: t`Always on`
            on: page.watch.alwaysOn
            onToggled: page.watch.alwaysOn = !page.watch.alwaysOn
        }
        Switch {
            theme: page.theme
            glyph: "spark"
            label: t`Glow`
            on: page.watch.bloom
            tint: page.theme.amber
            onToggled: page.watch.bloom = !page.watch.bloom
        }
    }
}
