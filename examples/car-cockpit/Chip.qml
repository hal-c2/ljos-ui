import Ljos.Ui
import Ljos.Core
import Ljos.Design
import "logic/glyphs.ts" as Glyphs

// A control on the touchscreen: a glyph, a label or both, at least a
// fingertip high. The one that is on glows; the rest stay flat. Tab
// reaches it and Space or Enter presses it, as a tap does.
Item {
    id: chip

    required property Theme theme
    property Glyphs.Mark glyph: ""
    property string label: ""
    property bool active: false
    property color tint: chip.theme.cyan
    // What a screen reader says it does, when the label alone does not.
    property string help: ""

    signal pressed()

    implicitWidth: Math.max(56, row.width + 32)
    implicitHeight: 56
    activeFocusOnTab: true

    Accessible.role: Accessible.Button
    Accessible.name: chip.label !== "" ? chip.label : chip.help
    Accessible.description: chip.help
    Accessible.checked: chip.active
    Accessible.onPressAction: chip.pressed()

    Keys.onPressed: (event: KeyEvent) => {
        if (event.key === Key.Space || event.key === Key.Return || event.key === Key.Enter) {
            chip.pressed()
            event.accepted = true
        }
    }

    scale: tap.pressed ? Motion.pressScale : 1
    PressBehavior on scale {}

    Rectangle {
        anchors.fill: parent
        radius: 16
        fill: chip.active ? chip.theme.selected : chip.theme.raised
        border.color: chip.theme.line
        border.width: 1
        Behavior on fill {
            ColorAnimation {
                duration: 100
                easing.type: Easing.OutCubic
            }
        }
    }

    // The ring of the one that is on.
    Neon {
        anchors.fill: parent
        theme: chip.theme
        tint: chip.tint
        spread: 0.6
        opacity: chip.active ? 1 : 0
        Behavior on opacity {
            NumberAnimation {
                duration: 200
                easing.type: Easing.OutCubic
            }
        }
        Rectangle {
            anchors.fill: parent
            radius: 16
            fill: "transparent"
            border.color: chip.tint
            border.width: 2
        }
    }

    // The keyboard's ring, outside the chip and above the glow, so a chip
    // that is on shows focus too.
    Rectangle {
        anchors.fill: parent
        anchors.margins: -4
        radius: 20
        fill: "transparent"
        border.color: chip.theme.text
        border.width: 2
        visible: chip.activeFocus
    }

    Item {
        id: row
        anchors.centerIn: parent
        width: (chip.glyph !== "" ? 26 : 0) + (chip.glyph !== "" && chip.label !== "" ? 10 : 0)
            + (chip.label !== "" ? caption.implicitWidth : 0)
        height: 26
        Icon {
            id: mark
            visible: chip.glyph !== ""
            width: 26
            height: 26
            glyph: chip.glyph
            color: chip.active ? chip.tint : chip.theme.text
        }
        Text {
            id: caption
            visible: chip.label !== ""
            anchors.verticalCenter: parent.verticalCenter
            x: chip.glyph !== "" ? 36 : 0
            text: chip.label
            color: chip.theme.text
            font.size: 17
            font.weight: chip.active ? Font.DemiBold : Font.Normal
        }
    }

    Tap {
        id: tap
        onTapped: chip.pressed()
    }
}
