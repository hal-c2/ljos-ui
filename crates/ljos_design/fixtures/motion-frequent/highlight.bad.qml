// expect: motion-frequent at 8
import QtQuick

ListView {
    width: 200
    height: 300
    model: 20
    highlightMoveDuration: 400
    delegate: Text {
        required property int index
        text: qsTr("Row %1").arg(index)
    }
}
