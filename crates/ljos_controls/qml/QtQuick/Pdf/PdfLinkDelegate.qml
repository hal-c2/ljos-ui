import QtQuick
import QtQuick.Controls
import QtQuick.Pdf

// The part of a page one link covers, for a delegate of a PdfLinkModel: it
// draws nothing, and says when it is clicked or a menu is asked for. The
// viewers decide what a click does.
Item {
    id: root

    required property var link
    required property rect rectangle
    required property url url
    required property int page
    required property point location
    required property real zoom

    signal tapped(var link)
    signal contextMenuRequested(var link)

    HoverHandler {
        id: hover
        cursorShape: Qt.PointingHandCursor
    }
    TapHandler {
        gesturePolicy: TapHandler.WithinBounds
        onTapped: root.tapped(root.link)
    }
    TapHandler {
        acceptedButtons: Qt.RightButton
        acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad | PointerDevice.Stylus
        gesturePolicy: TapHandler.WithinBounds
        onTapped: root.contextMenuRequested(root.link)
    }
    // A finger has no other button: holding it down asks for the menu.
    TapHandler {
        acceptedDevices: PointerDevice.TouchScreen
        onLongPressed: root.contextMenuRequested(root.link)
    }
    // A reader is told the page counted from 1, where the link's own text
    // counts from 0.
    ToolTip {
        visible: hover.hovered
        delay: 1000
        text: root.link.valid
            ? "Page " + (root.page + 1) + " location " + root.location.x.toFixed(1) + ", "
                + root.location.y.toFixed(1) + " zoom " + root.zoom
            : String(root.url)
    }
}
