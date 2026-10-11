// The Basic style of Ljós, written from the Qt Quick Controls documentation
// and from the sizes and colours a running Qt reports (CLEAN_ROOM.md).
import QtQuick
import QtQuick.Templates as T

T.Menu {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            contentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             contentHeight + topPadding + bottomPadding)

    margins: 0
    overlap: 1

    // What an `Action` or a `Menu` declared in the menu is shown as.
    delegate: MenuItem {}

    // The menu lays its items out in it.
    // TODO(listview): a list that scrolls when the menu is taller than
    // its window.
    contentItem: Item {}

    background: Rectangle {
        implicitWidth: 200
        implicitHeight: 40
        color: control.palette.window
        border.color: control.palette.dark
    }
}
