// expect: content-inconsistent-words at 11, content-inconsistent-words at 12
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 400; height: 200
    Column {
        spacing: 8
        Label { text: "Connection lost" }
        Button { text: "Try again" }
        Button { text: "Retry" }
    }
}
