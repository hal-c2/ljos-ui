// The Basic style of Ljós, written from the Qt Quick Controls documentation
// and from the sizes and colours a running Qt reports (CLEAN_ROOM.md).
import QtQuick
import QtQuick.Templates as T

T.AbstractMonthGrid {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    spacing: 6

    // The day, centred, of the month shown only: the days of the months
    // beside it are there to be counted, not read.
    delegate: Component {
        Text {
            required property var model

            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
            opacity: model.month === control.month ? 1 : 0
            text: model.day
            font: control.font
            color: control.palette.text
        }
    }

    // The template puts the days in it.
    contentItem: Component {
        Item {}
    }
}
