// expect: text-unbounded-dynamic at 11
import QtQuick
import QtQuick.Controls

Item {
    property string fileName
    width: 200
    height: 30
    Label {
        width: parent.width
        text: fileName
    }
}
