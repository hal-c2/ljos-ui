import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 360; height: 100
    Rectangle {
        anchors.centerIn: parent; width: 320; height: 40; color: palette.base
        Text { anchors.centerIn: parent; color: palette.text; text: "Task 7 finished: Build finished" }
    }
}
