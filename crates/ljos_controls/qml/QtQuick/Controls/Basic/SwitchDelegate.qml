// The Basic style of Ljós, written from the Qt Quick Controls documentation
// and from the sizes and colours a running Qt reports (CLEAN_ROOM.md).
import QtQuick
import QtQuick.Templates as T

T.SwitchDelegate {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding,
                             implicitIndicatorHeight + topPadding + bottomPadding)

    padding: 12
    spacing: 12

    // A row of a list takes no focus of its own (its view has the keyboard),
    // so it is not a stop for an assistive technology's focus either,
    // whatever its role says.
    Accessible.focusable: control.focusPolicy !== Qt.NoFocus

    // At the far end of the row: the right, or the left when mirrored.
    indicator: Component {
        Rectangle {
            implicitWidth: 56
            implicitHeight: 28
            x: control.mirrored ? control.leftPadding : control.width - width - control.rightPadding
            y: control.topPadding + (control.availableHeight - height) / 2
            radius: 8
            color: control.checked ? control.palette.dark : control.palette.midlight
            border.color: control.palette.highlight
            border.width: control.visualFocus ? 2 : 0

            Rectangle {
                x: control.visualPosition * (parent.width - width)
                width: 28
                height: 28
                radius: 16
                color: control.down ? control.palette.light : control.palette.window
                border.width: 1
                border.color: control.palette.mid
            }
        }
    }

    // The text, short of the indicator.
    contentItem: Component {
        Item {
            // What an application reads off the content item of a button.
            property alias text: label.text
            property alias font: label.font
            property alias color: label.color

            implicitWidth: label.implicitWidth + label.gap
            implicitHeight: label.implicitHeight

            Text {
                id: label
                readonly property real gap: control.indicator
                                            ? control.indicator.width + control.spacing : 0
                x: control.mirrored ? gap : 0
                width: parent.width - gap
                height: parent.height
                text: control.text
                font: control.font
                color: control.highlighted ? control.palette.highlightedText : control.palette.text
                elide: Text.ElideRight
                horizontalAlignment: control.mirrored ? Text.AlignRight : Text.AlignLeft
                verticalAlignment: Text.AlignVCenter
            }
        }
    }

    // A row shows only while something is going on in it.
    background: Component {
        Rectangle {
            implicitWidth: 100
            implicitHeight: 40
            visible: control.down || control.highlighted || control.visualFocus
            color: control.down ? control.palette.midlight : control.palette.light
        }
    }
}
