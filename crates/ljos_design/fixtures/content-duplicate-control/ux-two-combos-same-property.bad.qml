// expect: content-duplicate-control at 12
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 400; height: 100
    QtObject { id: settings; property int mode: 0 }
    Row {
        spacing: 8
        ComboBox { model: ["A", "B"]; currentIndex: settings.mode; onActivated: settings.mode = currentIndex }
        ComboBox { model: ["A", "B"]; currentIndex: settings.mode; onActivated: settings.mode = currentIndex }
    }
}
