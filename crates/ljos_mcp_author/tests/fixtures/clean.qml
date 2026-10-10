import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ApplicationWindow {
    id: window
    width: 480
    height: 320
    visible: true
    title: qsTr("Counter")

    property int count: 0

    function bump(by: int): int {
        const next = count + by
        count = next
        return next
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 12
        spacing: 8

        Label {
            Layout.fillWidth: true
            text: qsTr("Pressed %1 times").arg(window.count)
            font.pixelSize: 18
            Accessible.name: text
        }

        Button {
            text: qsTr("Add")
            onClicked: window.bump(1)
            Keys.onPressed: event => { if (event.key === Qt.Key_Space) window.bump(2) }
            Component.onCompleted: console.log("ready", count)
        }

        Image {
            source: "logo.png"
            Accessible.ignored: true
        }

        Repeater {
            model: 3
            delegate: Text {
                required property int index
                text: qsTr("Row ") + index
                Accessible.name: text
            }
        }
    }

    Timer {
        interval: 1000
        running: window.visible
        repeat: true
        onTriggered: window.count += 0
    }
}
