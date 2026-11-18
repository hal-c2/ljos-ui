import Ljos.Ui
import Ljos.Layout
import Ljos.Core
import Ljos.Design
import "logic/watch.ts" as W

// The phone's music, from the wrist: what plays, how far in, play and
// skip. The crown is the volume, drawn as an arc on the right edge.
WatchScreen {
    id: page

    readonly property real s: Math.min(page.width, page.height)
    // The progress ring: round the edge of a round screen; on a tall one
    // a circle below the song, clear of its words.
    readonly property real dial: page.round ? page.s - 16 : Math.min(page.width - 60, page.height - 240)
    readonly property real cy: page.round ? page.height / 2 : page.height / 2 + 70
    // The volume arc: inside the ring at the edge of a round screen, outside
    // it on a tall one, where the controls fill the ring.
    readonly property real volumeRadius: page.round ? page.dial / 2 - 26 : page.dial / 2 + 18

    // How far into the track, round the edge; it glows while it plays.
    Ring {
        x: (page.width - page.dial) / 2
        y: page.cy - page.dial / 2
        width: page.dial
        height: width
        theme: page.theme
        thickness: 6
        tint: page.theme.magenta
        value: page.watch.position / page.watch.nowPlaying.seconds
        lit: page.watch.playing
    }

    // The volume, at three o'clock, inside the progress ring.
    Shape {
        anchors.fill: parent
        ShapePath {
            strokeColor: page.theme.track
            strokeWidth: 6
            fillColor: "transparent"
            capStyle: ShapePath.RoundCap
            PathAngleArc {
                centerX: page.width / 2
                centerY: page.cy
                radiusX: page.volumeRadius
                radiusY: page.volumeRadius
                startAngle: 30
                sweepAngle: -60
            }
        }
        ShapePath {
            strokeColor: page.theme.cyan
            strokeWidth: 6
            fillColor: "transparent"
            capStyle: ShapePath.RoundCap
            PathAngleArc {
                centerX: page.width / 2
                centerY: page.cy
                radiusX: page.volumeRadius
                radiusY: page.volumeRadius
                startAngle: 30
                sweepAngle: -60 * page.watch.volume / 10
            }
        }
    }
    Item {
        anchors.fill: parent
        Accessible.role: Accessible.Slider
        Accessible.name: t`Volume`
        Accessible.description: t`${page.watch.volume} of 10`
    }

    Title {
        theme: page.theme
        y: page.round ? 64 : 44
        text: t`NOW PLAYING`
    }
    Text {
        id: song
        x: page.inset + 20
        width: page.width - 2 * (page.inset + 20)
        y: page.round ? 112 : 96
        text: page.watch.nowPlaying.title
        color: page.theme.text
        font.size: 26
        font.weight: Font.DemiBold
        align: Text.AlignHCenter
        elide: Text.ElideRight
    }
    Text {
        id: artist
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.top: song.bottom
        anchors.topMargin: 4
        text: page.watch.nowPlaying.artist
        color: page.theme.muted
        font.size: 17
    }
    Text {
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.top: artist.bottom
        anchors.topMargin: 6
        text: W.span(page.watch.position * 1000) + " / " + W.span(page.watch.nowPlaying.seconds * 1000)
        color: page.theme.muted
        font.size: 14
        font.features: { "tnum": 1 }
    }

    Row {
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.verticalCenter: parent.verticalCenter
        anchors.verticalCenterOffset: page.round ? 74 : 70
        spacing: 16
        Chip {
            Layout.alignment: Align.VCenter
            theme: page.theme
            glyph: "prev"
            help: t`Previous track`
            onPressed: page.watch.skip(-1)
        }
        Chip {
            width: 76
            height: 76
            theme: page.theme
            tint: page.theme.magenta
            glyph: page.watch.playing ? "pause" : "play"
            glyphSize: 30
            help: page.watch.playing ? t`Pause` : t`Play`
            primary: true
            onPressed: page.watch.playing ? page.watch.pause() : page.watch.play()
        }
        Chip {
            Layout.alignment: Align.VCenter
            theme: page.theme
            glyph: "next"
            help: t`Next track`
            onPressed: page.watch.skip(1)
        }
    }
}
