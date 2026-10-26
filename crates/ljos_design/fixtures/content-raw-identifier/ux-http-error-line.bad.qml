// expect: content-raw-identifier at 10
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 400; height: 100
    Label {
        anchors.centerIn: parent; color: palette.windowText
        text: "HTTP 502 https://api.example.test/v1/items?page=2"
    }
}
