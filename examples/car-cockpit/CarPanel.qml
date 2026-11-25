import Ljos.Ui
import Ljos.Core
import Ljos.Design
import Ljos.Layout
import "logic/car.ts" as Car

// The car itself: tap a door, the frunk, the trunk or the charge port to
// open or close it, on the 3D car or on its button. A part springs open
// past where it stops and settles, the car turns to show it, and its
// light strip flares. A locked or moving car keeps its parts shut and the
// panel says why.
Item {
    id: panel

    required property Theme theme
    required property Cockpit car

    // Why the last tap did nothing, or "".
    property string refused: ""
    // The part the car last turned to show.
    property Car.Part aimed: "doorLeft"

    function press(part: Car.Part) {
        if (panel.car.toggle(part) === true) {
            panel.refused = ""
            panel.aimed = part
            flare.restart()
        } else {
            panel.refused = panel.car.moving ? t`Stop the car first: nothing opens while it moves.`
                : panel.car.gear !== "P" ? t`Shift to P first: nothing opens out of park.`
                : t`The car is locked. Unlock it to open anything.`
            refusal.restart()
        }
    }

    // What the keyboard is on, said aloud.
    readonly property string names: panel.aimed === "doorLeft" ? t`Left door`
        : panel.aimed === "doorRight" ? t`Right door`
        : panel.aimed === "frunk" ? t`Frunk`
        : panel.aimed === "trunk" ? t`Trunk`
        : t`Charge port`

    // Where the camera stands to show each part, in degrees round the car.
    readonly property real view: panel.aimed === "doorLeft" ? 38
        : panel.aimed === "doorRight" ? -38
        : panel.aimed === "frunk" ? -24
        : panel.aimed === "chargePort" ? 150
        : 200

    // Each part's opening, 0 to 1. Opening overshoots and settles; closing
    // is quicker and lands softly.
    property real doorLeft: panel.car.doorLeft ? 1 : 0
    Behavior on doorLeft {
        NumberAnimation {
            duration: panel.car.doorLeft ? 500 : 350
            easing.type: panel.car.doorLeft ? Easing.OutBack : Easing.InOutCubic
        }
    }
    property real doorRight: panel.car.doorRight ? 1 : 0
    Behavior on doorRight {
        NumberAnimation {
            duration: panel.car.doorRight ? 500 : 350
            easing.type: panel.car.doorRight ? Easing.OutBack : Easing.InOutCubic
        }
    }
    property real frunk: panel.car.frunk ? 1 : 0
    Behavior on frunk {
        NumberAnimation {
            duration: panel.car.frunk ? 500 : 350
            easing.type: panel.car.frunk ? Easing.OutBack : Easing.InOutCubic
        }
    }
    property real trunk: panel.car.trunk ? 1 : 0
    Behavior on trunk {
        NumberAnimation {
            duration: panel.car.trunk ? 500 : 350
            easing.type: panel.car.trunk ? Easing.OutBack : Easing.InOutCubic
        }
    }
    property real chargePort: panel.car.chargePort ? 1 : 0
    Behavior on chargePort {
        NumberAnimation {
            duration: panel.car.chargePort ? 500 : 350
            easing.type: panel.car.chargePort ? Easing.OutBack : Easing.InOutCubic
        }
    }
    // The camera turns to what was tapped. With reduced motion it stays
    // where it is and a part is open at once, as Ljós has every number
    // that moves; its button's fill and ring still fade, so the change
    // shows.
    property real yaw: Motion.reduced ? 38 : panel.view
    Behavior on yaw {
        NumberAnimation {
            duration: 500
            easing.type: Easing.InOutCubic
        }
    }
    // The strips' brightness: a flare when something opens or closes.
    property real strips: 1
    SequentialAnimation {
        id: flare
        NumberAnimation {
            target: panel
            property: "strips"
            to: 2.2
            duration: 100
            easing.type: Easing.OutCubic
        }
        NumberAnimation {
            target: panel
            property: "strips"
            to: 1
            duration: 500
            easing.type: Easing.InOutCubic
        }
    }

    CarScene {
        id: scene
        anchors.fill: parent
        anchors.rightMargin: 272
        doorLeft: panel.doorLeft
        doorRight: panel.doorRight
        frunk: panel.frunk
        trunk: panel.trunk
        chargePort: panel.chargePort
        yaw: panel.yaw
        strips: panel.strips
        bloom: panel.theme.bloom
        dark: panel.theme.dark
        plinth: panel.theme.stage
        stripTint: panel.theme.dark ? "#00e5ff" : "#00b8cc"

        // Not the part's own name: that is its button's.
        aim: Car.nodeOfPart(panel.aimed)
        aimName: t`Car model: ${panel.names}`
        help: t`The car. Arrow keys choose a part; Space opens or closes it.`
        onAimed: (node: string) => {
            const part = Car.partOfNode(node)
            if (part !== undefined) {
                panel.aimed = part
            }
        }
        onPicked: (node: string) => {
            const part = Car.partOfNode(node)
            if (part !== undefined) {
                panel.press(part)
            }
        }
    }

    // What the car says when it will not open.
    Rectangle {
        id: notice
        anchors.horizontalCenter: scene.horizontalCenter
        y: 18
        width: Math.min(parent.width - 40, said.implicitWidth + 48)
        height: 48
        radius: 24
        fill: panel.theme.raised
        border.color: panel.theme.amber
        border.width: 1.5
        opacity: 0
        Accessible.role: Accessible.AlertMessage
        Accessible.name: panel.refused
        Text {
            id: said
            anchors.centerIn: parent
            width: parent.width - 48
            align: Text.AlignHCenter
            text: panel.refused
            color: panel.theme.text
            font.size: 16
            elide: Text.ElideRight
        }
        SequentialAnimation {
            id: refusal
            NumberAnimation {
                target: notice
                property: "opacity"
                to: 1
                duration: 150
                easing.type: Easing.OutCubic
            }
            PauseAnimation {
                duration: 2400
            }
            NumberAnimation {
                target: notice
                property: "opacity"
                to: 0
                duration: 350
                easing.type: Easing.InOutCubic
            }
        }
    }

    // The same parts as buttons, for whoever cannot or would rather not
    // aim at the model.
    // A column at the side, so the car has the panel's full height.
    Column {
        anchors.right: parent.right
        anchors.rightMargin: 24
        anchors.verticalCenter: parent.verticalCenter
        spacing: 8
        Chip {
            width: 228
            theme: panel.theme
            glyph: "door"
            label: t`Left door`
            active: panel.car.doorLeft
            onPressed: panel.press("doorLeft")
        }
        Chip {
            width: 228
            theme: panel.theme
            glyph: "door"
            label: t`Right door`
            active: panel.car.doorRight
            onPressed: panel.press("doorRight")
        }
        Chip {
            width: 228
            theme: panel.theme
            glyph: "car"
            label: t`Frunk`
            active: panel.car.frunk
            onPressed: panel.press("frunk")
        }
        Chip {
            width: 228
            theme: panel.theme
            glyph: "car"
            label: t`Trunk`
            active: panel.car.trunk
            onPressed: panel.press("trunk")
        }
        Chip {
            width: 228
            theme: panel.theme
            glyph: "bolt"
            label: t`Charge port`
            active: panel.car.chargePort
            tint: panel.theme.green
            onPressed: panel.press("chargePort")
        }
        Chip {
            width: 228
            theme: panel.theme
            glyph: panel.car.locked ? "lock" : "unlock"
            label: panel.car.locked ? t`Locked` : t`Unlocked`
            active: panel.car.locked
            tint: panel.theme.magenta
            onPressed: {
                if (!panel.car.locked) {
                    panel.car.closeAll()
                }
                panel.car.locked = !panel.car.locked
            }
        }
    }
}
