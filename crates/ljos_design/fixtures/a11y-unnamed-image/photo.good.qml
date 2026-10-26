import QtQuick

Item {
    Image {
        source: "photo.png"
        Accessible.name: qsTr("The team at the summit")
    }
    Image {
        source: "divider.png"
        Accessible.ignored: true
    }
}
