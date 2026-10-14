// The Basic style of Ljós, written from the Qt Quick Controls documentation
// and from the sizes and colours a running Qt reports (CLEAN_ROOM.md).
import QtQuick
import QtQuick.Templates as T

T.AbstractWeekNumberColumn {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    spacing: 6
    leftPadding: 6
    rightPadding: 6

    delegate: Component {
        Text {
            required property int weekNumber

            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
            text: weekNumber
            font: control.font
            color: control.palette.text
        }
    }

    // The template puts the numbers in it.
    contentItem: Component {
        Item {}
    }
}
