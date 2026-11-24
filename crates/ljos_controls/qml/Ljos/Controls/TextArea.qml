// Ljós's own style (docs/DIALECT.md §4.16): a field of many lines, drawn
// as a text field is, on the surface inside an edge in the input's
// colour that turns the focus colour, twice as wide, while it has the
// keys. In a scroll view or a flickable the area is the view's content,
// and an edge on it would scroll away with the text, so there it draws
// none: put the view in a Frame to give it one.
import QtQuick
import QtQuick.Templates as T

T.TextArea {
    id: control

    // Whether the area is the content of a view that scrolls it.
    readonly property bool scrolled: parent !== null && parent.parent !== null
                                     && parent.parent.contentItem === parent
                                     && parent.parent.contentY !== undefined

    implicitWidth: Math.max(contentWidth + leftPadding + rightPadding,
                            implicitBackgroundWidth + leftInset + rightInset,
                            placeholder.implicitWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(contentHeight + topPadding + bottomPadding,
                             implicitBackgroundHeight + topInset + bottomInset,
                             placeholder.implicitHeight + topPadding + bottomPadding)

    padding: Theme.spacing - 2
    leftPadding: Theme.spacing
    rightPadding: Theme.spacing
    hoverEnabled: true
    font.pixelSize: Theme.fontSize
    opacity: enabled ? 1 : 0.64

    color: Theme.text
    selectionColor: Theme.accent
    selectedTextColor: Theme.accentForeground
    placeholderTextColor: Theme.placeholder

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
            implicitHeight: Theme.controlHeight * 2
            visible: !control.scrolled
            radius: Theme.radius
            color: Theme.surface
            border.width: control.activeFocus ? 2 : 1
            border.color: control.activeFocus ? Theme.focus : Theme.input
        }
    }
}
