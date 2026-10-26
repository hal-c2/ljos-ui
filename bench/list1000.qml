// A ListView of 1000 rows of controls, still until it is scrolled: what
// a long list holds once it is shown and once it has been scrolled through.
import QtQuick
import QtQuick.Controls

ApplicationWindow {
    width: 400
    height: 600
    visible: true

    ListView {
        anchors.fill: parent
        model: 1000
        delegate: ItemDelegate {
            required property int index
            width: ListView.view.width
            text: "Row " + index
        }
        ScrollBar.vertical: ScrollBar {}
    }
}
