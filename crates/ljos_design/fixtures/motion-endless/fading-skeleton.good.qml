import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ProgressBar {
    indeterminate: true
    contentItem: Item {
        implicitWidth: 120; implicitHeight: 6
        Rectangle {
            width: 40; height: 6
            color: palette.highlight
            SequentialAnimation on opacity { loops: Animation.Infinite
                NumberAnimation { to: 0.5; duration: 1000 }
                NumberAnimation { to: 1; duration: 1000 }
            }
        }
    }
}
