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

    // This car, from behind and above.
    Neon {
        width: 150
        height: 70
        x: view.cx - 75
        y: view.height - 92
        theme: view.theme
        tint: view.theme.cyan
        Shape {
            anchors.fill: parent
            ShapePath {
                strokeColor: view.theme.cyan
                strokeWidth: 3
                fillColor: view.theme.surface
                joinStyle: ShapePath.RoundJoin
                PathSvg {
                    path: "M 8 62 L 14 30 C 18 18 28 10 44 8 L 106 8 C 122 10 132 18 136 30 L 142 62 Z"
                }
            }
            ShapePath {
                strokeColor: view.theme.magenta
                strokeWidth: 4
                fillColor: "transparent"
                capStyle: ShapePath.RoundCap
                PathSvg {
                    path: "M 22 46 L 128 46"
                }
            }
        }
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
            text: t`${Nav.metresTo(hint.turn, view.car.travelled)} m`
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
