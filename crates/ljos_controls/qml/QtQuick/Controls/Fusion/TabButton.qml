// The Fusion style of Ljós (CLEAN_ROOM.md), from the sizes and colours a
// running Qt reports. The current tab is the full height of the
// bar; the others start lower, behind it.
import QtQuick
import QtQuick.Controls.impl
import QtQuick.Controls.Fusion
import QtQuick.Templates as T

T.TabButton {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    padding: 2
    horizontalPadding: 4
    spacing: 6

    icon.width: 16
    icon.height: 16

    contentItem: Component {
        IconLabel {
            spacing: control.spacing
            mirrored: control.mirrored
            display: control.display
            text: control.text
            font: control.font
            color: control.palette.buttonText
        }
    }

    // The current tab is light; the others are darker, more so towards the
    // bottom (fusion-shades.feature).
    background: Component {
        Rectangle {
            id: frame
            y: control.checked ? 0 : 2
            implicitHeight: 21
            height: control.height - y
            border.color: Qt.darker(control.palette.window, 1.27)
            // The current tab is the tab frame's shade of the button colour;
            // the others darken from a button's to a groove's.
            readonly property color top: control.checked
                ? Qt.lighter(Fusion.tabFrameColor(control.palette), 1.04)
                : Qt.darker(Fusion.buttonColor(control.palette, false, false, false), 1.04)
            gradient: Gradient {
                GradientStop { position: 0; color: frame.top }
                GradientStop { position: control.checked ? 0 : 0.85; color: frame.top }
                GradientStop {
                    position: 1
                    color: control.checked ? Fusion.tabFrameColor(control.palette)
                                           : Fusion.grooveColor(control.palette)
                }
            }
        }
    }
}
