// The snippet uses two things on its widget, `focus` and `color`;
// this is Ljós's own widget that gives it those and shows where
// the focus is. Its look is not the documentation's.
import QtQuick

FocusScope {
    id: scope

    property alias color: face.color

    implicitWidth: 160
    implicitHeight: 30

    Rectangle {
        id: face

        anchors.fill: parent
        radius: 4
        border.width: scope.activeFocus ? 2 : 0
        border.color: "black"
    }

    MouseArea {
        anchors.fill: parent
        onClicked: scope.forceActiveFocus()
    }
}
