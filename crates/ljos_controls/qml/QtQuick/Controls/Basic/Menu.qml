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

    // The items are the rows of `contentModel`; the view scrolls them
    // only when the menu is lower than they are together, which is when
    // its window is. A menu that is not open is as high as its items.
    contentItem: Component {
        ListView {
            implicitHeight: contentHeight
            model: control.contentModel
            interactive: contentHeight + control.topPadding + control.bottomPadding > control.height
            clip: true
            currentIndex: control.currentIndex

            ScrollIndicator.vertical: ScrollIndicator {}
        }
    }

    background: Component {
        Rectangle {
            implicitWidth: 200
            implicitHeight: 40
            color: control.palette.window
            border.color: control.palette.dark
        }
    }
}
