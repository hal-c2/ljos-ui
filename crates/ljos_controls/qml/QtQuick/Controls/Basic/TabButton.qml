// The Basic style of Ljós, written from the Qt Quick Controls documentation
// and from the sizes and colours a running Qt reports (CLEAN_ROOM.md).
import QtQuick
import QtQuick.Controls.impl
import QtQuick.Templates as T

T.TabButton {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    padding: 6
    spacing: 6

    icon.width: 24
    icon.height: 24

    // The current tab is the colour of the window under the bar; the
    // others are darker, with light text.
    contentItem: Component {
        IconLabel {
            spacing: control.spacing
            mirrored: control.mirrored
            display: control.display
            text: control.text
            font: control.font
            color: control.checked ? control.palette.windowText : control.palette.brightText
        }
    }

    // Held down: half way from dark to mid.
    background: Component {
        Rectangle {
            implicitHeight: 40
            color: control.down
                   ? Qt.rgba(Math.round((control.palette.dark.r + control.palette.mid.r) * 127.5 + 0.001) / 255,
                             Math.round((control.palette.dark.g + control.palette.mid.g) * 127.5 + 0.001) / 255,
                             Math.round((control.palette.dark.b + control.palette.mid.b) * 127.5 + 0.001) / 255, 1)
                   : control.checked ? control.palette.window : control.palette.dark
        }
    }
}
