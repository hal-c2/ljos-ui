import QtQuick
import QtQuick.Layouts

Rectangle {
    colr: "red"
    width: 10
    anchors.fil: parent
    onClickd: console.log("never")
    Layout.fillWidht: true
    Text {
        font.pixelSze: 12
        text: "x"
    }
}
