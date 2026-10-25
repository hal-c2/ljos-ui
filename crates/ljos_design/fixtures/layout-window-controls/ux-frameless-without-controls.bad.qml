// expect: layout-window-controls at 8
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ApplicationWindow {
    width: 400; height: 300; visible: true
    flags: Qt.Window | Qt.FramelessWindowHint
    Label { anchors.centerIn: parent; text: "Content" }
}
