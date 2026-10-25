// expect: text-raw-markup at 14
// audit: scheme=light
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 360; height: 100
    Rectangle {
        anchors.centerIn: parent; width: 320; height: 40; color: palette.base
        Text {
            anchors.centerIn: parent; color: palette.text
            textFormat: Text.PlainText
            text: "<task-result id=\"7\">Build finished</task-result>"
        }
    }
}
