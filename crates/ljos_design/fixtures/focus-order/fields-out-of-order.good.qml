import QtQuick
import QtQuick.Controls

Rectangle {
    width: 400; height: 300
    color: palette.window
    Column {
        x: 20; y: 20
        spacing: 8
        TextField { placeholderText: "Name" }
        TextField { placeholderText: "Street" }
        TextField { placeholderText: "City" }
        Button { text: "Done" }
    }
}
