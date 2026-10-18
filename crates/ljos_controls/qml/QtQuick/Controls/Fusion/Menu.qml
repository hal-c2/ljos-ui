// The Fusion style of Ljós (CLEAN_ROOM.md), from the sizes and colours a
// running Qt reports: a menu in the base colour inside a border darker
// than the window.
import QtQuick
import QtQuick.Controls.Fusion
import QtQuick.Templates as T

T.Menu {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            contentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             contentHeight + topPadding + bottomPadding)

    margins: 0
    padding: 1
    overlap: 2

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
            implicitHeight: 20
            color: control.palette.base
            border.color: Qt.darker(control.palette.window, 1.4)

            // A faint shadow a pixel down and to the right.
            Rectangle {
                x: 1
                y: 1
                z: -1
                width: parent.width
                height: parent.height
                color: control.palette.shadow
                opacity: 0.2
            }
        }
    }
}
