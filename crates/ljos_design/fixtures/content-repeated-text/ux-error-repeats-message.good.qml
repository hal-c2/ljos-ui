import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 400; height: 160
    Column {
        spacing: 4
        Label { text: "The export could not be finished." }
        Label { text: "The disk is full. Free some space and retry." }
    }
}
