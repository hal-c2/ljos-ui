import Ljos.Ui
import Ljos.Core
import Ljos.Design
import "logic/health.ts" as Health

// The heart rate written large with a heart that beats once per sample,
// and the last samples drawn as a line whose newest point glows.
Item {
    id: pulse

    required property Theme theme
    required property Watch watch
    property real chartHeight: 96

    implicitHeight: big.height + 14 + pulse.chartHeight

    // One beat per sample. The heart grows and its glow comes up; with
    // reduced motion, which takes what moves to its end at once, the glow
    // is what is left of the beat.
    property real beat: 0
    readonly property int sampled: pulse.watch.samples
    onSampledChanged: {
        if (pulse.visible) {
            thump.restart()
            glow.restart()
        }
    }
    SequentialAnimation {
        id: thump
        NumberAnimation {
            target: pulse
            property: "beat"
            to: 1
            duration: 110
            easing.type: Easing.OutCubic
        }
        NumberAnimation {
            target: pulse
            property: "beat"
            to: 0
            duration: 380
            easing.type: Easing.InOutCubic
        }
    }
    NumberAnimation {
        id: glow
        target: heart
        property: "opacity"
        from: 0.45
        to: 1
        duration: 380
        easing.type: Easing.OutCubic
    }

    Item {
        id: big
        anchors.horizontalCenter: parent.horizontalCenter
        width: heart.width + 12 + number.implicitWidth + 8 + unit.implicitWidth
        height: number.implicitHeight
        Neon {
            id: heart
            width: 40
            height: 40
            anchors.verticalCenter: parent.verticalCenter
            theme: pulse.theme
            tint: pulse.theme.magenta
            spread: 0.5
            scale: 1 + pulse.beat * 0.18
            Icon {
                anchors.fill: parent
                glyph: "heart"
                color: pulse.theme.magenta
                weight: 2.2
            }
        }
        Text {
            id: number
            x: heart.width + 12
            text: `${pulse.watch.bpm}`
            color: pulse.theme.text
            font.size: 64
            font.weight: Font.DemiBold
            font.features: { "tnum": 1 }
        }
        Text {
            id: unit
            anchors.left: number.right
            anchors.leftMargin: 8
            anchors.baseline: number.baseline
            text: t`BPM`
            color: pulse.theme.muted
            font.size: 16
            font.letterSpacing: 2
        }
    }

    Item {
        id: chart
        anchors.top: big.bottom
        anchors.topMargin: 14
        width: pulse.width
        height: pulse.chartHeight
        Accessible.role: Accessible.Chart
        Accessible.name: t`Heart rate, the last ${pulse.watch.historyKeep} seconds`

        Shape {
            anchors.fill: parent
            ShapePath {
                strokeColor: pulse.theme.line
                strokeWidth: 1
                fillColor: "transparent"
                PathSvg {
                    path: "M 0 " + (chart.height / 2) + " L " + chart.width + " " + (chart.height / 2)
                }
            }
            ShapePath {
                strokeColor: pulse.theme.cyan
                strokeWidth: 2.5
                fillColor: "transparent"
                capStyle: ShapePath.RoundCap
                joinStyle: ShapePath.RoundJoin
                PathSvg {
                    path: Health.chart(pulse.watch.history, chart.width, chart.height, 40, 180, pulse.watch.historyKeep)
                }
            }
        }
        // The newest sample: the one key highlight that glows.
        Neon {
            x: chart.width - 7
            y: chart.height - Math.max(0, Math.min(1, (pulse.watch.bpm - 40) / 140)) * chart.height - 7
            width: 14
            height: 14
            visible: pulse.watch.samples > 0
            theme: pulse.theme
            tint: pulse.theme.cyan
            spread: 0.8
            Rectangle {
                anchors.fill: parent
                radius: 7
                fill: pulse.theme.cyan
            }
        }
    }
}
