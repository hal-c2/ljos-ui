// expect: content-raw-identifier at 11
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 500; height: 100
    ListView {
        anchors.fill: parent
        model: ["/home/user/projects/app/src/main.rs"]
        delegate: ItemDelegate { width: ListView.view.width; text: modelData }
    }
}
