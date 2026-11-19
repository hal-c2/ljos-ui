import Ljos.Ui
import Ljos.Core
import Ljos.Layout
import Ljos.Design
import "logic/media.ts" as Media

// The player: what is playing with a visualiser under it, the controls,
// and the list to pick from. The tracks are made up for the demo.
Item {
    id: panel

    required property Theme theme
    required property Cockpit car

    readonly property int bars: 36
    // Round a five-second loop while music plays. With reduced motion the
    // bars hold still at a level that shows the music is on.
    property real phase: 0
    NumberAnimation on phase {
        from: 0
        to: 1
        duration: 5000
        loops: Animation.Infinite
        easing.type: Easing.Linear
        running: panel.car.playing && !Motion.reduced && panel.visible
    }
    readonly property real energy: panel.car.playing ? 1 : 0.18
    Behavior on energy {
        NumberAnimation {
            duration: 350
            easing.type: Easing.InOutCubic
        }
    }

    readonly property color coverTint: panel.car.track % 4 === 0 ? panel.theme.cyan
        : panel.car.track % 4 === 1 ? panel.theme.magenta
        : panel.car.track % 4 === 2 ? panel.theme.amber
        : panel.theme.green

    // ----- Now playing --------------------------------------------------

    Item {
        id: playing
        anchors.left: parent.left
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        anchors.margins: 32
        width: parent.width * 0.6

        // The cover: drawn, not an image. Rings in the track's colour.
        Rectangle {
            id: cover
            width: 200
            height: 200
            radius: 22
            fill: panel.theme.sunken
            border.color: panel.theme.line
            border.width: 1
            clip: true
            Neon {
                anchors.fill: parent
                theme: panel.theme
                tint: panel.coverTint
                spread: 0.7
                Shape {
                    anchors.fill: parent
                    ShapePath {
                        strokeColor: panel.coverTint
                        strokeWidth: 3
                        fillColor: "transparent"
                        PathSvg {
                            path: [0, 1, 2, 3, 4].map((ring: number) => {
                                const r = 18 + ring * 17 + (panel.car.track * 5) % 13
                                return `M ${100 - r} 130 A ${r} ${r} 0 0 1 ${100 + r} 130`
                            }).join(" ")
                        }
                    }
                    ShapePath {
                        strokeColor: panel.theme.magenta
                        strokeWidth: 3
                        fillColor: "transparent"
                        capStyle: ShapePath.RoundCap
                        PathSvg {
                            path: "M 20 160 L 180 160"
                        }
                    }
                }
            }
        }

        Column {
            anchors.left: cover.right
            anchors.leftMargin: 28
            anchors.right: parent.right
            y: 16
            spacing: 6
            Text {
                width: parent.width
                text: panel.car.nowPlaying.title
                color: panel.theme.text
                font.size: 34
                font.weight: Font.DemiBold
                elide: Text.ElideRight
            }
            Text {
                width: parent.width
                text: panel.car.nowPlaying.artist
                color: panel.theme.muted
                font.size: 20
                elide: Text.ElideRight
            }
            Item {
                width: 1
                height: 18
            }
            Row {
                spacing: 12
                Chip {
                    theme: panel.theme
                    glyph: "prev"
                    help: t`Previous track`
                    onPressed: panel.car.skip(-1)
                }
                Chip {
                    width: 72
                    theme: panel.theme
                    glyph: panel.car.playing ? "pause" : "play"
                    help: panel.car.playing ? t`Pause` : t`Play`
                    active: panel.car.playing
                    tint: panel.theme.magenta
                    onPressed: panel.car.playing = !panel.car.playing
                }
                Chip {
                    theme: panel.theme
                    glyph: "next"
                    help: t`Next track`
                    onPressed: panel.car.skip(1)
                }
            }
        }

        // Progress.
        Item {
            id: progress
            anchors.left: parent.left
            anchors.right: parent.right
            y: 224
            height: 24
            Accessible.role: Accessible.ProgressBar
            Accessible.name: t`${Media.clock(panel.car.position)} of ${Media.clock(panel.car.nowPlaying.seconds)}`
            Rectangle {
                y: 9
                width: parent.width
                height: 6
                radius: 3
                fill: panel.theme.track
            }
            Neon {
                anchors.fill: parent
                theme: panel.theme
                tint: panel.theme.magenta
                spread: 0.5
                Rectangle {
                    y: 9
                    width: Math.max(6, parent.width * panel.car.position / panel.car.nowPlaying.seconds)
                    height: 6
                    radius: 3
                    fill: panel.theme.magenta
                }
            }
        }
        Text {
            anchors.left: parent.left
            anchors.top: progress.bottom
            text: Media.clock(panel.car.position)
            color: panel.theme.muted
            font.size: 14
            font.features: { "tnum": 1 }
        }
        Text {
            anchors.right: parent.right
            anchors.top: progress.bottom
            text: Media.clock(panel.car.nowPlaying.seconds)
            color: panel.theme.muted
            font.size: 14
            font.features: { "tnum": 1 }
        }

        // The visualiser.
        Item {
            id: bars
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            height: parent.height - 290
            Accessible.role: Accessible.Graphic
            Accessible.name: panel.car.playing ? t`Visualiser, moving with the music` : t`Visualiser, at rest`
            Neon {
                anchors.fill: parent
                theme: panel.theme
                tint: panel.theme.cyan
                spread: 0.6
                Repeater {
                    model: panel.bars
                    delegate: Rectangle {
                        id: bar
                        required property int index
                        readonly property real slot: bars.width / panel.bars
                        x: bar.index * bar.slot + 2
                        width: bar.slot - 4
                        height: Math.max(4, bars.height * Media.level(bar.index, panel.bars, panel.phase, panel.energy))
                        anchors.bottom: parent.bottom
                        radius: 2
                        fill: bar.index < panel.bars * 0.66 ? panel.theme.cyan : panel.theme.magenta
                    }
                }
            }
        }
    }

    // ----- The list -------------------------------------------------------

    Rectangle {
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        anchors.margins: 24
        anchors.left: playing.right
        anchors.leftMargin: 32
        radius: 20
        fill: panel.theme.sunken
        border.color: panel.theme.line
        border.width: 1

        ListView {
            id: list
            anchors.fill: parent
            anchors.margins: 8
            clip: true
            model: Media.tracks
            delegate: Item {
                id: row
                required property int index
                required property Media.Track modelData
                readonly property bool current: row.index === panel.car.track
                width: list.width
                height: 60
                activeFocusOnTab: true
                Accessible.role: Accessible.Button
                Accessible.name: t`${row.modelData.title}, ${row.modelData.artist}`
                Accessible.onPressAction: row.choose()
                Keys.onPressed: (event: KeyEvent) => {
                    if (event.key === Key.Space || event.key === Key.Return) {
                        row.choose()
                        event.accepted = true
                    }
                }
                function choose() {
                    panel.car.track = row.index
                    panel.car.position = 0
                    panel.car.playing = true
                }
                Rectangle {
                    anchors.fill: parent
                    radius: 12
                    fill: row.current ? panel.theme.selected : "transparent"
                    border.color: row.activeFocus ? panel.theme.text : "transparent"
                    border.width: 2
                }
                Text {
                    x: 16
                    y: 9
                    width: parent.width - 90
                    text: row.modelData.title
                    color: row.current ? panel.theme.cyan : panel.theme.text
                    font.size: 18
                    font.weight: row.current ? Font.DemiBold : Font.Normal
                    elide: Text.ElideRight
                }
                Text {
                    x: 16
                    y: 33
                    width: parent.width - 90
                    text: row.modelData.artist
                    color: panel.theme.muted
                    font.size: 14
                    elide: Text.ElideRight
                }
                Text {
                    anchors.right: parent.right
                    anchors.rightMargin: 16
                    anchors.verticalCenter: parent.verticalCenter
                    text: Media.clock(row.modelData.seconds)
                    color: panel.theme.muted
                    font.size: 14
                    font.features: { "tnum": 1 }
                }
                Tap {
                    onTapped: row.choose()
                }
            }
        }
    }
}
