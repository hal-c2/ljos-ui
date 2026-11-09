// Ljós's own style (docs/DIALECT.md §4.16): a raised field with an edge
// of 3:1 that turns to the focus colour while it has the keyboard.
import QtQuick
import QtQuick.Templates as T

T.TextField {
    id: control

    implicitWidth: implicitBackgroundWidth + leftInset + rightInset
                   || Math.max(contentWidth, placeholder.implicitWidth) + leftPadding + rightPadding
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             contentHeight + topPadding + bottomPadding,
                             placeholder.implicitHeight + topPadding + bottomPadding)

    padding: Theme.spacing
    topPadding: 0
    bottomPadding: 0
    hoverEnabled: true
    font.pixelSize: Theme.fontSize
    opacity: enabled ? 1 : 0.64

    color: Theme.text
    selectionColor: Theme.accent
    selectedTextColor: Theme.accentForeground
    placeholderTextColor: Theme.placeholder
    verticalAlignment: TextInput.AlignVCenter

    // The first child: the background comes after it and lies behind.
    Text {
        id: placeholder
        x: control.leftPadding
        y: control.topPadding
        width: control.width - (control.leftPadding + control.rightPadding)
        height: control.height - (control.topPadding + control.bottomPadding)

        text: control.placeholderText
        font: control.font
        color: control.placeholderTextColor
        verticalAlignment: control.verticalAlignment
        horizontalAlignment: control.horizontalAlignment
        visible: !control.length && !control.preeditText
                 && (!control.activeFocus || control.horizontalAlignment !== Qt.AlignHCenter)
        elide: Text.ElideRight
    }

    background: Component {
        Rectangle {
            implicitWidth: 200
            implicitHeight: Theme.controlHeight
            radius: Theme.radius
            color: Theme.surfaceRaised
            border.width: control.activeFocus ? 2 : 1
            border.color: control.activeFocus ? Theme.focus : Theme.input
        }
    }
}
