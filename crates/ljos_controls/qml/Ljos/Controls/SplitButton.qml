// Ljós's own style (docs/DIALECT.md §4.16): a split button, an action and
// a menu of its other options in one outlined part. The action's half
// says `clicked`; the chevron's half opens `menu` below it where there is
// one, and says `menuRequested` either way. `compact` leaves the action
// its icon alone, still named by `text`.
import QtQuick
import QtQuick.Templates as T

T.Control {
    id: control

    property string text: ""
    property string iconName: ""
    property bool compact: false
    property bool actionEnabled: true
    property bool menuEnabled: true
    property T.Popup menu: null
    // What a reader says for the chevron, which has no text of its own.
    property string menuName: text.length > 0 ? qsTr("More %1 options").arg(text)
                                              : qsTr("More options")

    signal clicked()
    signal menuRequested()

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    padding: 0
    font.pixelSize: Theme.fontSize
    font.weight: Font.Medium
    opacity: enabled ? 1 : 0.64

    function openMenu() {
        if (menu) {
            menu.parent = control
            // Under the chevron's end, and kept there as the button's width
            // changes. A popover places itself by its edge.
            if (menu.side === undefined) {
                menu.x = Qt.binding(() => control.mirrored ? 0 : control.width - menu.width)
                menu.y = Qt.binding(() => control.height + Theme.spacingSmall)
            }
            menu.open()
        }
        control.menuRequested()
    }

    contentItem: Component {
        Item {
            implicitWidth: action.implicitWidth + 1 + more.implicitWidth
            implicitHeight: Theme.controlHeight

            // Each half is a button of the style, so each is reached by the
            // keys, draws its own ring and gives under a press. They sit
            // inside the outline, a pixel from its edge.
            Button {
                id: action
                objectName: "action"
                x: control.mirrored ? parent.width - width : 0
                y: 1
                // Inside the outline, so a pixel shorter at each edge than a
                // button of its own, and that is its size too.
                implicitHeight: Theme.controlHeight - 2
                // All the chevron leaves, so a split button made wider is
                // all action up to the seam.
                width: parent.width - 1 - more.width
                height: parent.height - 2
                subtle: true
                text: control.compact ? "" : control.text
                iconName: control.iconName
                font: control.font
                enabled: control.actionEnabled
                Accessible.name: control.text
                onClicked: control.clicked()
            }

            Rectangle {
                objectName: "seam"
                x: control.mirrored ? more.width : action.width
                y: 1
                width: 1
                height: parent.height - 2
                color: Theme.input
            }

            Button {
                id: more
                objectName: "more"
                x: control.mirrored ? 0 : parent.width - width
                y: 1
                implicitWidth: Theme.controlHeight - 4
                implicitHeight: Theme.controlHeight - 2
                height: parent.height - 2
                padding: (implicitWidth - Theme.iconSize) / 2
                subtle: true
                iconName: "chevron-down"
                enabled: control.menuEnabled
                Accessible.name: control.menuName
                onClicked: control.openMenu()
            }
        }
    }

    background: Component {
        Rectangle {
            implicitWidth: Theme.controlHeight * 2
            implicitHeight: Theme.controlHeight
            radius: Theme.radius
            color: Theme.surface
            border.width: 1
            border.color: Theme.input
        }
    }
}
