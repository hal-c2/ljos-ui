// expect: text-font-literal at 8
import QtQuick
import QtQuick.Controls

Item {
    Label {
        text: qsTr("Total")
        font.family: "Helvetica"
    }
}
