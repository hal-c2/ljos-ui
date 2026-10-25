import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 320; height: 200
    ColumnLayout {
        spacing: 8
        ComboBox { model: ["A", "B"]; Layout.preferredWidth: 200 }
        ComboBox { model: ["C", "D"]; Layout.preferredWidth: 200 }
        ComboBox { model: ["E", "F"]; Layout.preferredWidth: 200 }
    }
}
