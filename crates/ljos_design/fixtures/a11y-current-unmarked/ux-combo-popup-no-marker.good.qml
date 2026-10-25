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
            font.bold: combo.currentIndex === index
            highlighted: combo.highlightedIndex === index
        }
        Component.onCompleted: popup.open()
    }
}
