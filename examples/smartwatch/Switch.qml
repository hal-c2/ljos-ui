import Ljos.Ui
import Ljos.Core
import Ljos.Design
import "logic/glyphs.ts" as Glyphs

// A round switch with its name under it. On, it fills with its tint.
Item {
    id: sw

    required property Theme theme
    property Glyphs.Mark glyph: ""
    property string label: ""
    property bool on: false
    property color tint: sw.theme.cyan

    signal toggled()

    width: 108
    height: 104

    Chip {
        id: key
        anchors.horizontalCenter: parent.horizontalCenter
        width: 72
        height: 72
        theme: sw.theme
        tint: sw.tint
        glyph: sw.glyph
        glyphSize: 30
        help: sw.label
        active: sw.on
        primary: sw.on
        onPressed: {
            sw.toggled()
        }
    }
    Text {
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.top: key.bottom
        anchors.topMargin: 6
        text: sw.label
        color: sw.on ? sw.theme.text : sw.theme.muted
        font.size: 14
    }
}
