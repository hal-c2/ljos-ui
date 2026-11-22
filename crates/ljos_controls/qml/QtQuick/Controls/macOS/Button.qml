// The macOS style of Ljós (CLEAN_ROOM.md), written from the Qt Quick
// Controls documentation of the macOS style and from Apple's public
// Human Interface Guidelines for push buttons and toolbar items.
//
// A push button is a small rounded rectangle; a flat button, the
// toolbar item's shape, has no plate of its own until it is held down,
// checked or highlighted. Icons are named with `icon.name`; Ljós has
// no icon theme and no SF Symbols, so a name shows nothing and
// `icon.source` is shown instead when given (as in every style).
import QtQuick
import QtQuick.Controls.impl
import QtQuick.Templates as T

T.Button {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    padding: 6
    horizontalPadding: padding + 6
    spacing: 6

    icon.width: 20
    icon.height: 20

    contentItem: Component {
        IconLabel {
            spacing: control.spacing
            mirrored: control.mirrored
            display: control.display
            text: control.text
            font: control.font
            // A flat button is drawn on the window, not on its background.
            color: control.checked || control.highlighted ? control.palette.brightText
                 : !control.flat || control.down ? control.palette.buttonText
                 : control.palette.windowText
        }
    }

    background: Component {
        Rectangle {
            // A flat button has a plate only while held down, checked or
            // highlighted; its focus ring shows without one, on the window.
            readonly property bool plate: !control.flat || control.down
                                          || control.checked || control.highlighted
            implicitWidth: 64
            implicitHeight: 21
            radius: 6
            visible: plate || control.visualFocus
            color: !plate ? "transparent"
                 : control.checked || control.highlighted ? control.palette.dark
                 : control.down ? control.palette.mid
                 : control.palette.button
            border.color: control.palette.highlight
            border.width: control.visualFocus ? 2 : 0
        }
    }
}
