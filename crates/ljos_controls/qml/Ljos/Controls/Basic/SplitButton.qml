// Ljos.Controls.Basic's split button (docs/DIALECT.md §4.16): the same
// action and menu as Ljos.Controls' (its file says how they behave), as
// two of the Basic style's buttons side by side.
import QtQuick
import QtQuick.Templates as T
import QtQuick.Controls.Basic as Basic

T.Control {
    id: control

    property string text: ""
    property string iconName: ""
    property bool compact: false
    property bool actionEnabled: true
    property bool menuEnabled: true
    property T.Popup menu: null
    property string menuName: text.length > 0 ? qsTr("More %1 options").arg(text)
                                              : qsTr("More options")

    signal clicked()
    signal menuRequested()

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    spacing: 1

    function openMenu() {
        if (menu) {
            menu.parent = control
            // Under the chevron's end, and kept there as the button's width
            // changes. A popover places itself by its edge.
            if (menu.side === undefined) {
                menu.x = Qt.binding(() => control.mirrored ? 0 : control.width - menu.width)
                menu.y = Qt.binding(() => control.height)
            }
            menu.open()
        }
        control.menuRequested()
    }

    contentItem: Component {
        Row {
            spacing: control.spacing
            layoutDirection: control.mirrored ? Qt.RightToLeft : Qt.LeftToRight

            // The Basic style has no icons of its own; `iconName` is kept
            // so a file moves between the modules by its import alone, and
            // a compact one still shows its text.
            Basic.Button {
                objectName: "action"
                text: control.text
                font: control.font
                enabled: control.actionEnabled
                Accessible.name: control.text
                onClicked: control.clicked()
            }

            Basic.Button {
                objectName: "more"
                text: "▾"
                font: control.font
                enabled: control.menuEnabled
                Accessible.name: control.menuName
                onClicked: control.openMenu()
            }
        }
    }
}
