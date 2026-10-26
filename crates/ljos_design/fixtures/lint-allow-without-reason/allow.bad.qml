// expect: lint-allow-without-reason at 8
import QtQuick
import QtQuick.Controls

Item {
    Label {
        text: qsTr("Total")
        // ljos-allow(text-font-literal)
        font.family: "monospace"
    }
}
