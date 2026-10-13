// The Basic style of Ljós, written from the Qt Quick Controls documentation
// and from the sizes and colours a running Qt reports (CLEAN_ROOM.md).
import QtQuick
import QtQuick.Templates as T

T.SplitView {
    id: control

    // A handle is 6 across, as long as the view, and darker while it is
    // held.
    handle: Component {
        Rectangle {
            implicitWidth: control.orientation === Qt.Horizontal ? 6 : control.width
            implicitHeight: control.orientation === Qt.Horizontal ? control.height : 6
            color: T.SplitHandle.pressed ? control.palette.mid
                : (T.SplitHandle.hovered ? control.palette.midlight : control.palette.button)
        }
    }
}
