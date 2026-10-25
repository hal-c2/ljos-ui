// expect: layout-offscreen at 13
// audit: size=400x300
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 400; height: 120
    TabBar {
        width: 400
        Repeater {
            model: ["General", "Appearance", "Keyboard", "Network", "Advanced"]
            TabButton { text: modelData; width: 120 }
        }
    }
}
