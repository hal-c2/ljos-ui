// expect: layout-misaligned at 12
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 320; height: 200
    Column {
        spacing: 8
        ComboBox { model: ["A", "B"]; width: 200 }
        ComboBox { model: ["C", "D"]; width: 200 }
        ComboBox { model: ["E", "F"]; width: 240 }
    }
}
