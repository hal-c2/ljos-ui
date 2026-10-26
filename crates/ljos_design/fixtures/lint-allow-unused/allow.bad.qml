// expect: lint-allow-unused at 8
import QtQuick
import QtQuick.Controls

Item {
    Label {
        text: qsTr("Total")
        // ljos-allow(text-font-literal): the figures line up in columns
        font.bold: true
    }
}
