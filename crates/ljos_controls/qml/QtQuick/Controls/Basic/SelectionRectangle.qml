// The Basic style of Ljós, written from the Qt Quick Controls documentation
// and from the sizes and colours a running Qt reports (CLEAN_ROOM.md).
import QtQuick
import QtQuick.Templates as T

// A handle is a circle 28 across on the window colour, framed in mid.
T.SelectionRectangle {
    id: control

    topLeftHandle: Component {
        Rectangle {
            width: 28
            height: 28
            radius: 14
            color: control.palette.window
            border.width: 1
            border.color: control.palette.mid
        }
    }

    bottomRightHandle: Component {
        Rectangle {
            width: 28
            height: 28
            radius: 14
            color: control.palette.window
            border.width: 1
            border.color: control.palette.mid
        }
    }
}
