import Ljos.Ui
import Ljos.Core
import Ljos.Design
import "logic/glyphs.ts" as Glyphs

// A control on the watch: a glyph, a label or both, a fingertip high at
// least. A pill, or a circle with only a glyph. The one that is on glows;
// the rest stay flat. Tab reaches it and Space or Enter presses it.
Item {
    id: chip

    required property Theme theme
    property Glyphs.Mark glyph: ""
    property string label: ""
    property bool active: false
    // The key action of its screen: filled with the accent.
    property bool primary: false
    property color tint: chip.theme.cyan
    // What a screen reader says it is, when the label alone does not.
    property string help: ""
    property real glyphSize: 24

    signal pressed()

    implicitWidth: chip.label === "" ? 56 : Math.max(56, row.width + 36)
    implicitHeight: 56
    activeFocusOnTab: true

    Accessible.role: Accessible.Button
    Accessible.name: chip.help !== "" ? chip.help : chip.label
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
        radius: Math.min(chip.width, chip.height) / 2
        fill: chip.primary ? chip.tint : chip.active ? chip.theme.selected : chip.theme.raised
        border.color: chip.activeFocus ? chip.theme.text : chip.primary ? chip.tint : chip.theme.line
        border.width: chip.activeFocus ? 2 : 1
        Behavior on fill {
            ColorAnimation {
                duration: Motion.enterSmallFrequent
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
                duration: Motion.enterSmall
                easing.type: Easing.OutCubic
            }
        }
        Rectangle {
            anchors.fill: parent
            radius: Math.min(chip.width, chip.height) / 2
            fill: "transparent"
            border.color: chip.tint
            border.width: 2
        }
    }

    Item {
        id: row
        anchors.centerIn: parent
        width: (chip.glyph !== "" ? chip.glyphSize : 0) + (chip.glyph !== "" && chip.label !== "" ? 8 : 0)
            + (chip.label !== "" ? caption.implicitWidth : 0)
        height: chip.glyphSize
        Icon {
            visible: chip.glyph !== ""
            width: chip.glyphSize
            height: chip.glyphSize
            glyph: chip.glyph
            color: chip.primary ? chip.theme.onAccent : chip.active ? chip.tint : chip.theme.text
        }
        Text {
            id: caption
            visible: chip.label !== ""
            anchors.verticalCenter: parent.verticalCenter
            x: chip.glyph !== "" ? chip.glyphSize + 8 : 0
            text: chip.label
            color: chip.primary ? chip.theme.onAccent : chip.theme.text
            font.size: 17
            font.weight: chip.primary || chip.active ? Font.DemiBold : Font.Normal
        }
    }

    Tap {
        id: tap
        onTapped: chip.pressed()
    }
}
