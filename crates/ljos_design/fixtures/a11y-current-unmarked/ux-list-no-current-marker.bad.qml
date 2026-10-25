// expect: a11y-current-unmarked at 12
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 300; height: 200
    ListView {
        anchors.fill: parent
        currentIndex: 1
        model: ["Light", "Dark", "High contrast"]
        delegate: ItemDelegate {
            width: ListView.view.width
            text: modelData
        }
    }
}
