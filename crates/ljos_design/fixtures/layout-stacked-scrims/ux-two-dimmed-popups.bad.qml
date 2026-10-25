// expect: layout-stacked-scrims at 9
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 400; height: 300
    Popup { modal: true; dim: true; Component.onCompleted: open(); Label { text: "First" } }
    Popup { modal: true; dim: true; Component.onCompleted: open(); Label { text: "Second" } }
}
