// audit: reduced-motion=false
import QtQuick

Rectangle {
    width: 300; height: 200
    Rectangle {
        width: 40; height: 40
        color: "steelblue"
        NumberAnimation on x {
            from: 0; to: 200
            duration: 1000
            loops: Animation.Infinite
        }

    }
}
