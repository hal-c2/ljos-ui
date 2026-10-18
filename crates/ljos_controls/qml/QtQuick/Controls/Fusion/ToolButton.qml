// The Fusion style of Ljós (CLEAN_ROOM.md), drawn with its parts in
// QtQuick.Controls.Fusion.impl: drawings of our own, fitted to what qml6 shows.
import QtQuick
import QtQuick.Controls.impl
import QtQuick.Controls.Fusion.impl
import QtQuick.Templates as T

T.ToolButton {
    // Not `control`: inside a part's component that names the part's
    // own property, which Fusion's parts have.
    id: host

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    padding: 6
    spacing: 6

    icon.width: 16
    icon.height: 16

    contentItem: Component {
        IconLabel {
            spacing: host.spacing
            mirrored: host.mirrored
            display: host.display
            text: host.text
            font: host.font
            color: host.palette.buttonText
        }
    }

    // A row of them reads as a bar: the button shows only when it is used.
    background: Component {
        ButtonPanel {
            implicitWidth: 20
            implicitHeight: 20
            control: host
            visible: host.down || host.checked || host.highlighted || host.visualFocus
                     || host.enabled && host.hovered
            pressed: host.down || host.checked
            highlighted: host.highlighted
        }
    }
}
