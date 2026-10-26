import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 400; height: 100
    Label {
        anchors.centerIn: parent; color: palette.windowText
        text: "The server could not be reached. Try again in a moment."
    }
}
