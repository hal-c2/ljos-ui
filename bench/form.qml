// A form of 50 Qt Quick Controls in ten rows: the cost of controls,
// each of which is many items.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Rectangle {
    width: 1000
    height: 640
    color: "white"

    GridLayout {
        anchors.fill: parent
        anchors.margins: 8
        columns: 5

        Repeater {
            model: 10
            delegate: TextField {
                required property int index
                Layout.row: index
                Layout.column: 0
                placeholderText: "Field " + index
            }
        }
        Repeater {
            model: 10
            delegate: CheckBox {
                required property int index
                Layout.row: index
                Layout.column: 1
                text: "Option " + index
                checked: index % 2
            }
        }
        Repeater {
            model: 10
            delegate: Slider {
                required property int index
                Layout.row: index
                Layout.column: 2
                value: index / 10
            }
        }
        Repeater {
            model: 10
            delegate: SpinBox {
                required property int index
                Layout.row: index
                Layout.column: 3
                value: index
            }
        }
        Repeater {
            model: 10
            delegate: ComboBox {
                required property int index
                Layout.row: index
                Layout.column: 4
                model: ["One", "Two", "Three"]
                currentIndex: index % 3
            }
        }
    }
}
