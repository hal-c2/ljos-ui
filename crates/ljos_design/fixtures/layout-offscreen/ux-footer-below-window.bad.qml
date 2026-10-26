// expect: layout-offscreen at 16
// audit: size=400x300
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 400; height: 300
    Column {
        width: parent.width
        ListView {
            width: parent.width; height: 300
            model: 20
            delegate: ItemDelegate { width: ListView.view.width; text: "Item " + index }
        }
        Row { spacing: 8; Button { text: "Cancel" } Button { text: "OK" } }
    }
}
