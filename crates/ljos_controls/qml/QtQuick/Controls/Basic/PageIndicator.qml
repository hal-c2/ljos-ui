// The Basic style of Ljós, written from the Qt Quick Controls documentation
// and from the sizes and colours a running Qt reports (CLEAN_ROOM.md).
import QtQuick
import QtQuick.Templates as T

T.PageIndicator {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    padding: 6
    spacing: 6

    // A dot per page: the current one strongest, a pressed one between.
    delegate: Component {
        Rectangle {
            required property int index

            implicitWidth: 8
            implicitHeight: 8
            radius: width / 2
            color: control.palette.dark
            opacity: index === control.currentIndex ? 0.95 : pressed ? 0.7 : 0.45

            Behavior on opacity { NumberAnimation { duration: 100 } }
        }
    }

    // The template puts the dots in it.
    contentItem: Component {
        Row {
            spacing: control.spacing
        }
    }
}
