import QtQuick

Item {
    width: 200; height: 100
    Loader {
        anchors.fill: parent
        sourceComponent: Rectangle { color: "#336699" }
    }
}
