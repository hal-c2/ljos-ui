// Ljós's own style (docs/DIALECT.md §4.16): the buttons at the foot of a
// dialog, at its end, in the style's own buttons. The one that accepts
// the dialog is filled, so the way forward is the one that stands out;
// one that destroys says so.
import QtQuick
import QtQuick.Templates as T

T.DialogButtonBox {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    spacing: Theme.spacing
    padding: Theme.spacing * 2
    alignment: Qt.AlignRight

    delegate: Button {
        primary: DialogButtonBox.buttonRole === DialogButtonBox.AcceptRole
                 || DialogButtonBox.buttonRole === DialogButtonBox.YesRole
        destructive: DialogButtonBox.buttonRole === DialogButtonBox.DestructiveRole
    }

    background: null
}
