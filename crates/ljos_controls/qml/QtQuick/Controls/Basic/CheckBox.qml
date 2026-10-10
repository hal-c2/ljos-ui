// The Basic style of Ljós, written from the Qt Quick Controls documentation
// and from the sizes and colours a running Qt reports (CLEAN_ROOM.md).
import QtQuick
import QtQuick.Templates as T

T.CheckBox {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding,
                             implicitIndicatorHeight + topPadding + bottomPadding)

    padding: 6
    spacing: 6

    // Beside the text, or in the middle when there is none.
    indicator: Rectangle {
        implicitWidth: 28
        implicitHeight: 28
        x: control.text.length > 0
           ? (control.mirrored ? control.width - width - control.rightPadding : control.leftPadding)
           : control.leftPadding + (control.availableWidth - width) / 2
        y: control.topPadding + (control.availableHeight - height) / 2
        color: control.down ? control.palette.light : control.palette.base
        border.width: 1
        border.color: control.palette.mid
        // TODO(focus): a wider border in `palette.highlight` with visual focus.

        // The mark is a filled square: a neutral drawing of our own.
        Rectangle {
            x: (parent.width - width) / 2
            y: (parent.height - height) / 2
            width: 14
            height: 14
            radius: 2
            color: control.palette.text
            visible: control.checkState === 2
        }

        Rectangle {
            x: (parent.width - width) / 2
            y: (parent.height - height) / 2
            width: 16
            height: 3
            color: control.palette.text
            visible: control.checkState === 1
        }
    }

    // The text, with room for the indicator on the side it is on.
    contentItem: Item {
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
            x: control.mirrored ? 0 : gap
            width: parent.width - gap
            height: parent.height
            text: control.text
            font: control.font
            color: control.palette.windowText
            elide: Text.ElideRight
            verticalAlignment: Text.AlignVCenter
        }
    }
}
