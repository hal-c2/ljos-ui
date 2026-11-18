import Ljos.Ui
import Ljos.Core
import "logic/drive.ts" as Drive
import "logic/nav.ts" as Nav

// What the driving assist sees: the lane ahead in perspective, the car in
// front when there is one, the speed limit and the gear. The lane lines
// glow while the assist holds the lane, in Drive.
Item {
    id: view

    required property Theme theme
    required property Cockpit car

    readonly property bool assisting: view.car.gear === "D"
    readonly property real horizon: 26
    readonly property real cx: view.width / 2
    // How far the car sits off the lane's centre, in pixels at the bottom.
    property real drift: view.assisting ? view.car.current.drift * 70 : 0
    Behavior on drift {
        NumberAnimation {
            duration: 450
            easing.type: Easing.InOutCubic
        }
    }

    // The neighbouring lanes, unlit.
    Shape {
        anchors.fill: parent
        ShapePath {
            strokeColor: view.theme.line
            strokeWidth: 2
            fillColor: "transparent"
            strokeStyle: ShapePath.DashLine
            dashPattern: [6, 6]
            PathSvg {
                path: `M ${view.cx - 520 - view.drift} ${view.height} L ${view.cx - 60} ${view.horizon} M ${view.cx + 520 - view.drift} ${view.height} L ${view.cx + 60} ${view.horizon}`
            }
        }
    }

    // The lane the car is in.
    Neon {
        anchors.fill: parent
        theme: view.theme
        tint: view.assisting ? view.theme.cyan : view.theme.muted
        spread: 0.8
        Shape {
            anchors.fill: parent
            ShapePath {
                strokeColor: view.assisting ? view.theme.cyan : view.theme.muted
                strokeWidth: 4
                fillColor: "transparent"
                capStyle: ShapePath.RoundCap
                PathSvg {
                    path: `M ${view.cx - 190 - view.drift} ${view.height} L ${view.cx - 22} ${view.horizon} M ${view.cx + 190 - view.drift} ${view.height} L ${view.cx + 22} ${view.horizon}`
                }
            }
        }
    }

    // The car ahead: higher and smaller the further it is.
    Item {
        id: lead
        readonly property real gap: view.car.current.lead
        readonly property real near: lead.gap > 0 ? 1 - Math.min(60, lead.gap) / 70 : 0
        width: 46 + 70 * lead.near
        height: lead.width * 0.62
        x: view.cx - lead.width / 2 - view.drift * lead.near
        y: view.horizon + (view.height - 150 - view.horizon) * lead.near
        opacity: view.assisting && lead.gap > 0 ? 1 : 0
        Behavior on opacity {
            NumberAnimation {
                duration: 250
                easing.type: Easing.OutCubic
            }
        }
        Behavior on y {
            NumberAnimation {
                duration: 500
                easing.type: Easing.InOutCubic
            }
        }
        Rectangle {
            anchors.fill: parent
            radius: lead.height * 0.3
            fill: view.theme.raised
            border.color: view.theme.muted
            border.width: 1.5
        }
        Neon {
            anchors.fill: parent
            theme: view.theme
            tint: view.theme.red
            spread: 0.6
            Rectangle {
                x: lead.width * 0.1
                y: lead.height * 0.32
                width: lead.width * 0.22
                height: Math.max(3, lead.height * 0.12)
                radius: height / 2
                fill: view.theme.red
            }
            Rectangle {
                x: lead.width * 0.68
                y: lead.height * 0.32
                width: lead.width * 0.22
                height: Math.max(3, lead.height * 0.12)
                radius: height / 2
                fill: view.theme.red
            }
        }
    }

    // This car, from behind and above: the same model the car panel shows.
    ChaseView {
        objectName: "chaseCar"
        width: 340
        height: 170
        x: view.cx - 170
        y: view.height - 170
        bloom: view.theme.bloom
        dark: view.theme.dark
        stripTint: view.theme.cyan
        tailTint: view.theme.magenta
        sway: -view.drift * 0.08
        Accessible.role: Accessible.Graphic
        Accessible.name: t`Your car`
    }

    // The limit of the road.
    Rectangle {
        x: 12
        y: 8
        width: 58
        height: 58
        radius: 29
        fill: view.theme.signFace
        border.color: view.theme.red
        border.width: 5
        Accessible.role: Accessible.StaticText
        Accessible.name: t`Speed limit ${Drive.toUnit(view.car.current.limit, view.car.miles)}`
        Text {
            anchors.centerIn: parent
            text: `${Drive.toUnit(view.car.current.limit, view.car.miles)}`
            color: view.theme.signInk
            font.size: 20
            font.weight: Font.Bold
        }
    }

    // The next turn of the route.
    Item {
        id: hint
        readonly property Nav.Turn turn: Nav.nextTurn(view.car.travelled)
        readonly property Nav.Short ahead: Nav.shortDistance(Nav.metresTo(hint.turn, view.car.travelled), view.car.miles)
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.topMargin: 8
        width: 170
        height: 58
        Icon {
            id: turnIcon
            anchors.left: parent.left
            anchors.verticalCenter: parent.verticalCenter
            width: 30
            height: 30
            glyph: hint.turn.kind
            color: view.theme.cyan
        }
        Text {
            anchors.left: turnIcon.right
            anchors.leftMargin: 10
            anchors.right: parent.right
            y: 8
            text: t`${hint.ahead.amount} ${hint.ahead.unit}`
            color: view.theme.text
            font.size: 18
            font.weight: Font.DemiBold
            elide: Text.ElideRight
        }
        Text {
            anchors.left: turnIcon.right
            anchors.leftMargin: 10
            anchors.right: parent.right
            y: 32
            text: hint.turn.street
            color: view.theme.muted
            font.size: 13
            elide: Text.ElideRight
        }
    }
}
