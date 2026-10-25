// expect: a11y-current-unmarked at 12
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 300; height: 260
    ComboBox {
        id: combo
        model: ["Small", "Medium", "Large"]
        currentIndex: 1
        delegate: ItemDelegate {
            width: combo.width
            text: modelData
        }
        Component.onCompleted: popup.open()
    }
}
