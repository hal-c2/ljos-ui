// The Fusion style of Ljós (CLEAN_ROOM.md), drawn with its parts in
// QtQuick.Controls.Fusion.impl: drawings of our own, fitted to what qml6 shows.
import QtQuick
import QtQuick.Controls.Fusion.impl
import QtQuick.Templates as T

T.TextField {
    // Not `control`: inside a part's component that names the part's
    // own property, which Fusion's parts have.
    id: host

    implicitWidth: implicitBackgroundWidth + leftInset + rightInset
                   || Math.max(contentWidth, placeholder.implicitWidth) + leftPadding + rightPadding
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             contentHeight + topPadding + bottomPadding,
                             placeholder.implicitHeight + topPadding + bottomPadding)

    padding: 4

    color: host.palette.text
    selectionColor: host.palette.highlight
    selectedTextColor: host.palette.highlightedText
    placeholderTextColor: host.palette.placeholderText
    verticalAlignment: TextInput.AlignVCenter

    Text {
        id: placeholder
        x: host.leftPadding
        y: host.topPadding
        width: host.width - (host.leftPadding + host.rightPadding)
        height: host.height - (host.topPadding + host.bottomPadding)

        text: host.placeholderText
        font: host.font
        color: host.placeholderTextColor
        verticalAlignment: host.verticalAlignment
        visible: !host.length && !host.preeditText
                 && (!host.activeFocus || host.horizontalAlignment !== Qt.AlignHCenter)
        elide: Text.ElideRight
        renderType: host.renderType
    }

    background: Component {
        TextFieldBackground {
            control: host
        }
    }
}
