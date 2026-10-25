import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 400; height: 200
    Column {
        spacing: 8
        Label { text: "Connection lost" }
        Button { text: "Retry" }
        Button { text: "Retry" }
    }
}
