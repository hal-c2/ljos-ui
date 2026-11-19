import Ljos.Ui
import Ljos.Core
import "logic/nav.ts" as Nav
import "logic/drive.ts" as Drive

// The map: a made-up city drawn from logic/nav.ts, the route lit, the car
// on it, and the next turn in a card. The map is drawn on a 1000 by 640
// plane and scaled to the panel.
Item {
    id: panel

    required property Theme theme
    required property Cockpit car

    clip: true

    readonly property Nav.Fix fix: Nav.fixAt(panel.car.travelled)
    readonly property Nav.Turn turn: Nav.nextTurn(panel.car.travelled)
    // How far the turn is, in the driver's units.
    readonly property Nav.Short ahead: Nav.shortDistance(Nav.metresTo(panel.turn, panel.car.travelled), panel.car.miles)
    // Kilometres left to the route's end.
    readonly property real remaining: (1 - panel.car.travelled) * Nav.routeMetres() / 1000
    // The map fills the panel's width; the panel shows a band of it that
    // follows the car, so the car stays in sight on a wide, short screen.
    readonly property real fit: panel.width / 1000
    readonly property real follow: Math.min(0, Math.max(panel.height - 640 * panel.fit,
                                                          panel.height * 0.55 - panel.fix.y * panel.fit))

    Rectangle {
        anchors.fill: parent
        fill: panel.theme.sunken
    }

    Item {
        id: map
        width: 1000
        height: 640
        transformOrigin: Item.TopLeft
        scale: panel.fit
        y: panel.follow
        Behavior on y {
            NumberAnimation {
                duration: 500
                easing.type: Easing.InOutSine
            }
        }
        Accessible.role: Accessible.Graphic
        Accessible.name: t`Map: the route to ${Nav.nextTurn(1).street}`

        Shape {
            anchors.fill: parent
            // Parks.
            ShapePath {
                strokeColor: "transparent"
                fillColor: panel.theme.park
                PathSvg {
                    path: Nav.parks.join(" ")
                }
            }
            // Water.
            ShapePath {
                strokeColor: panel.theme.water
                strokeWidth: 46
                fillColor: "transparent"
                capStyle: ShapePath.FlatCap
                PathSvg {
                    path: Nav.river
                }
            }
            // Lanes.
            ShapePath {
                strokeColor: panel.theme.track
                strokeWidth: 6
                fillColor: "transparent"
                PathSvg {
                    path: Nav.lanes.join(" ")
                }
            }
            // Avenues.
            ShapePath {
                strokeColor: panel.theme.line
                strokeWidth: 12
                fillColor: "transparent"
                PathSvg {
                    path: Nav.avenues.join(" ")
                }
            }
        }

        // The route.
        Neon {
            anchors.fill: parent
            theme: panel.theme
            tint: panel.theme.cyan
            spread: 0.7
            Shape {
                anchors.fill: parent
                ShapePath {
                    strokeColor: panel.theme.cyan
                    strokeWidth: 7
                    fillColor: "transparent"
                    capStyle: ShapePath.RoundCap
                    joinStyle: ShapePath.RoundJoin
                    PathSvg {
                        path: Nav.routePath()
                    }
                }
            }
        }

        // The car: it glides between the drive's one-second steps.
        Item {
            id: marker
            width: 0
            height: 0
            x: panel.fix.x
            y: panel.fix.y
            rotation: panel.fix.heading
            Behavior on x {
                NumberAnimation {
                    duration: 500
                    easing.type: Easing.InOutSine
                }
            }
            Behavior on y {
                NumberAnimation {
                    duration: 500
                    easing.type: Easing.InOutSine
                }
            }
            Rectangle {
                x: -22
                y: -22
                width: 44
                height: 44
                radius: 22
                fill: panel.theme.surface
                border.color: panel.theme.line
                border.width: 1
            }
            Neon {
                x: -16
                y: -16
                width: 32
                height: 32
                theme: panel.theme
                tint: panel.theme.magenta
                spread: 0.6
                Shape {
                    anchors.fill: parent
                    ShapePath {
                        strokeColor: panel.theme.magenta
                        strokeWidth: 3
                        fillColor: panel.theme.magenta
                        joinStyle: ShapePath.RoundJoin
                        // An arrowhead pointing along +x: the heading.
                        PathSvg {
                            path: "M 28 16 L 6 6 L 11 16 L 6 26 Z"
                        }
                    }
                }
            }
        }
    }

    // The next turn.
    Rectangle {
        id: card
        x: 24
        y: 24
        width: 340
        height: 112
        radius: 20
        fill: panel.theme.raised
        border.color: panel.theme.line
        border.width: 1
        objectName: "nextTurn"
        Accessible.role: Accessible.StaticText
        // One whole sentence per unit, each counted by plural, so a
        // translator sees the full phrase and the language's own forms.
        Accessible.name: panel.ahead.unit === "m"
            ? t`In ${plural(panel.ahead.amount, { one: "# metre", other: "# metres" })}, ${panel.turn.kind} onto ${panel.turn.street}`
            : panel.ahead.unit === "ft"
            ? t`In ${plural(panel.ahead.amount, { one: "# foot", other: "# feet" })}, ${panel.turn.kind} onto ${panel.turn.street}`
            : t`In ${plural(panel.ahead.amount, { one: "# mile", other: "# miles" })}, ${panel.turn.kind} onto ${panel.turn.street}`
        Neon {
            x: 20
            y: 24
            width: 64
            height: 64
            theme: panel.theme
            tint: panel.theme.cyan
            spread: 0.5
            Icon {
                anchors.fill: parent
                glyph: panel.turn.kind
                color: panel.theme.cyan
                weight: 2.2
            }
        }
        Text {
            x: 104
            y: 20
            width: card.width - 120
            text: t`${panel.ahead.amount} ${panel.ahead.unit}`
            color: panel.theme.text
            font.size: 32
            font.weight: Font.DemiBold
            font.features: { "tnum": 1 }
            elide: Text.ElideRight
        }
        Text {
            x: 104
            y: 66
            width: card.width - 120
            text: panel.turn.street
            color: panel.theme.muted
            font.size: 18
            elide: Text.ElideRight
        }
    }

    // How far is left and when the car gets there.
    Rectangle {
        id: arrival
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.margins: 24
        width: 300
        height: 76
        radius: 20
        fill: panel.theme.raised
        border.color: panel.theme.line
        border.width: 1
        readonly property real km: panel.remaining
        Accessible.role: Accessible.StaticText
        Accessible.name: t`${Drive.toUnit(arrival.km, panel.car.miles)} ${panel.car.miles ? "miles" : "kilometres"} to go`
        Text {
            x: 22
            anchors.verticalCenter: parent.verticalCenter
            width: parent.width - 44
            text: t`${(panel.car.miles ? arrival.km * 0.621371 : arrival.km).toFixed(1)} ${panel.car.miles ? "mi" : "km"} to ${Nav.nextTurn(1).street}`
            color: panel.theme.text
            font.size: 18
            elide: Text.ElideRight
        }
    }
}
