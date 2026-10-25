// expect: focus-indicator-missing at 10
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 300; height: 100
    ComboBox {
        model: ["Small", "Large"]
        background: Item {}
    }
}
