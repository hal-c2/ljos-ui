import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 400; height: 120
    TabBar {
        id: bar
        width: 400
        Repeater {
            model: ["General", "Appearance", "Keyboard", "Network", "Advanced"]
            TabButton { text: modelData; width: bar.width / 5; font.pixelSize: 11 }
        }
    }
}
